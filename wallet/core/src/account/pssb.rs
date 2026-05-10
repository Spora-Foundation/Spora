//!
//! Tools for interfacing wallet accounts with pssbs.
//! (Partial Signed Spora Transaction Bundles).
//!

pub use crate::error::Error;
use crate::imports::*;
use crate::tx::PaymentOutput;
use crate::tx::PaymentOutputs;
use futures::stream;
use secp256k1::schnorr;
use secp256k1::{Message, PublicKey};
use spora_addresses::{Address, Prefix};
use spora_bip32::{DerivationPath, KeyFingerprint, PrivateKey};
use spora_consensus_client::{CellEntry as ClientCellEntry, CellEntryReference};
use spora_consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync};
use spora_consensus_core::tx::{push_data_script, Script, VerifiableTransaction};
use spora_wallet_core::tx::{DataKind, Generator, GeneratorSettings, PaymentDestination, PendingTransaction};
use spora_wallet_psst::bundle::unlock_cell_outputs_as_batch_transaction_pssb;
pub use spora_wallet_psst::bundle::Bundle;
use spora_wallet_psst::prelude::{Finalizer, Inner, KeySource, SignInputOk, Signature, Signer};
pub use spora_wallet_psst::psst::{Creator, PSST};

struct PSSBSignerInner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    keys: Mutex<AHashMap<Address, [u8; 32]>>,
}

pub struct PSSBSigner {
    inner: Arc<PSSBSignerInner>,
}

impl PSSBSigner {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self { inner: Arc::new(PSSBSignerInner { keydata, account, payment_secret, keys: Mutex::new(AHashMap::new()) }) }
    }

    pub fn ingest(&self, addresses: &[Address]) -> Result<()> {
        let mut keys = self.inner.keys.lock()?;

        // Skip addresses that are already present in the key map.
        let addresses = addresses.iter().filter(|a| !keys.contains_key(a)).collect::<Vec<_>>();
        if !addresses.is_empty() {
            let private_keys = self.inner.account.clone().create_address_private_keys(
                &self.inner.keydata,
                &self.inner.payment_secret,
                addresses.as_slice(),
            )?;

            for (address, private_key) in private_keys {
                keys.insert(address.clone(), private_key.to_bytes());
            }
        }
        Ok(())
    }

    fn public_key(&self, for_address: &Address) -> Result<PublicKey> {
        let keys = self.inner.keys.lock()?;
        match keys.get(for_address) {
            Some(private_key) => {
                let kp = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key)?;
                Ok(kp.public_key())
            }
            None => Err(Error::from("PSSBSigner address coverage error")),
        }
    }

    fn sign_schnorr(&self, for_address: &Address, message: Message) -> Result<schnorr::Signature> {
        let keys = self.inner.keys.lock()?;
        match keys.get(for_address) {
            Some(private_key) => {
                let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key)?;
                Ok(schnorr_key.sign_schnorr(message))
            }
            None => Err(Error::from("PSSBSigner address coverage error")),
        }
    }
}

pub struct PSSTGenerator {
    generator: Generator,
    signer: Arc<PSSBSigner>,
    prefix: Prefix,
}

impl PSSTGenerator {
    pub fn new(generator: Generator, signer: Arc<PSSBSigner>, prefix: Prefix) -> Self {
        Self { generator, signer, prefix }
    }

    pub fn stream(&self) -> impl Stream<Item = Result<PSST<Signer>, Error>> {
        PSSTStream::new(self.generator.clone(), self.signer.clone(), self.prefix)
    }
}

struct PSSTStream {
    generator_stream: Pin<Box<dyn Stream<Item = Result<PendingTransaction, Error>> + Send>>,
    signer: Arc<PSSBSigner>,
    prefix: Prefix,
}

impl PSSTStream {
    fn new(generator: Generator, signer: Arc<PSSBSigner>, prefix: Prefix) -> Self {
        let generator_stream = generator.stream().map_err(Error::from);
        Self { generator_stream: Box::pin(generator_stream), signer, prefix }
    }
}

impl Stream for PSSTStream {
    type Item = Result<PSST<Signer>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_ref();

        let _prefix = this.prefix;
        let _signer = this.signer.clone();

        match self.get_mut().generator_stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(pending_tx))) => {
                let psst = convert_pending_tx_to_psst(pending_tx);
                Poll::Ready(Some(psst))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn convert_pending_tx_to_psst(pending_tx: PendingTransaction) -> Result<PSST<Signer>, Error> {
    let signable_tx = pending_tx.signable_transaction();
    let verifiable_tx = signable_tx.as_verifiable();
    let cell_tx = pending_tx.transaction();
    let mut inputs_with_entries = Vec::with_capacity(cell_tx.inputs.len());
    for (index, input) in cell_tx.inputs.iter().cloned().enumerate() {
        let entry = verifiable_tx
            .cell_entry(index)
            .cloned()
            .ok_or_else(|| Error::Custom(format!("PSST conversion requires resolved Cell metadata for input {index}")))?;
        inputs_with_entries.push((input, entry));
    }
    let psst_inner = Inner::try_from((cell_tx, inputs_with_entries))?;
    Ok(PSST::<Signer>::from(psst_inner))
}

pub async fn bundle_from_psst_generator(generator: PSSTGenerator) -> Result<Bundle, Error> {
    let mut bundle: Bundle = Bundle::new();
    let mut stream = generator.stream();

    while let Some(psst_result) = stream.next().await {
        match psst_result {
            Ok(psst) => bundle.add_psst(psst),
            Err(e) => return Err(e),
        }
    }

    Ok(bundle)
}

pub async fn pssb_signer_for_address(
    bundle: &Bundle,
    signer: Arc<PSSBSigner>,
    _network_id: NetworkId,
    sign_for_address: Option<&Address>,
    derivation_path: Option<DerivationPath>,
    key_fingerprint: Option<KeyFingerprint>,
) -> Result<Bundle, Error> {
    let mut signed_bundle = Bundle::new();
    let _reused_values = SigHashReusedValuesUnsync::new();

    // If sign_for_address is provided, we'll use it for all signatures
    // Otherwise, collect addresses per PSST
    let addresses_per_psst: Vec<Vec<Address>> = if sign_for_address.is_some() {
        // Create a vec of single-address vecs
        bundle.iter().map(|_| vec![sign_for_address.unwrap().clone()]).collect()
    } else {
        return Err(Error::Custom(
            "automatic PSST signer address inference was removed with CellEntry.address; pass sign_for_address explicitly".to_string(),
        ));
    };

    // Prepare the signer with all unique addresses
    let all_addresses: Vec<Address> = addresses_per_psst.iter().flat_map(|addresses| addresses.iter().cloned()).collect();
    signer.ingest(all_addresses.as_slice())?;

    // in case of keypair account, we don't have a derivation path,
    // so we need to skip the key source
    let mut key_source = None;
    if let Some(key_fingerprint) = key_fingerprint {
        if let Some(derivation_path) = derivation_path {
            key_source = Some(KeySource { key_fingerprint, derivation_path: derivation_path.clone() });
        }
    }

    // Process each PSST in the bundle
    for (psst_idx, psst_inner) in bundle.iter().cloned().enumerate() {
        let psst: PSST<Signer> = PSST::from(psst_inner);
        let current_addresses = &addresses_per_psst[psst_idx];

        // Create new reused values for each PSST
        let _reused_values = SigHashReusedValuesUnsync::new();

        let sign = |signer_psst: PSST<Signer>| -> Result<PSST<Signer>, Error> {
            signer_psst
                .pass_signature_sync(|tx, sighash| -> Result<Vec<SignInputOk>, String> {
                    tx.tx
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(input_idx, _input)| {
                            let hash =
                                calc_schnorr_signature_hash(&tx.as_verifiable(), input_idx, sighash[input_idx], &_reused_values);
                            let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).map_err(|e| e.to_string())?;

                            // Get the appropriate address for this input
                            let address = if let Some(sign_addr) = sign_for_address {
                                sign_addr
                            } else {
                                current_addresses.get(input_idx).ok_or_else(|| format!("No address found for input {}", input_idx))?
                            };

                            let pub_key = signer.public_key(address).map_err(|e| format!("Failed to get public key: {}", e))?;

                            let signature = signer.sign_schnorr(address, msg).map_err(|e| format!("Failed to sign: {}", e))?;

                            Ok(SignInputOk { signature: Signature::Schnorr(signature), pub_key, key_source: key_source.clone() })
                        })
                        .collect()
                })
                .map_err(Error::from)
        };

        let signed_psst = sign(psst)?;
        signed_bundle.add_psst(signed_psst);
    }

    Ok(signed_bundle)
}

pub fn finalize_psst_one_or_more_sig_and_witness_template(psst: PSST<Finalizer>) -> Result<PSST<Finalizer>, Error> {
    let result = psst.finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>, String> {
        Ok(inner
            .inputs
            .iter()
            .map(|input| -> Result<Vec<u8>, String> {
                let mut finalized = Vec::new();
                for (_, signature) in input.partial_sigs.clone() {
                    let mut signature_bytes = Vec::from(signature.into_bytes());
                    signature_bytes.push(input.sighash_type.to_u8());
                    finalized.extend(push_data_script(&signature_bytes).map_err(|e| e.to_string())?);
                }

                if let Some(witness_template) = input.witness_template.as_ref() {
                    finalized.extend(push_data_script(witness_template.as_slice()).map_err(|e| e.to_string())?);
                }

                Ok(finalized)
            })
            .collect::<Result<Vec<_>, _>>()?)
    });

    match result {
        Ok(finalized_psst) => Ok(finalized_psst),
        Err(e) => Err(Error::from(e.to_string())),
    }
}

pub fn bundle_to_finalizer_stream(bundle: &Bundle) -> impl Stream<Item = Result<PSST<Finalizer>, Error>> + Send {
    stream::iter(bundle.iter().cloned().collect::<Vec<_>>()).map(move |psst_inner| {
        let psst: PSST<Creator> = PSST::from(psst_inner);
        let psst_finalizer = psst.constructor().updater().signer().finalizer();
        finalize_psst_one_or_more_sig_and_witness_template(psst_finalizer)
    })
}

pub fn psst_to_pending_transaction(
    finalized_psst: PSST<Finalizer>,
    network_id: NetworkId,
    change_address: Address,
    source_cell_context: Option<CellContext>,
) -> Result<PendingTransaction, Error> {
    let (cell_entries_ref, aggregate_input_value, first_output) = {
        let inner_psst = finalized_psst.deref();
        let (cell_entries_ref, aggregate_input_value): (Vec<CellEntryReference>, u64) = inner_psst
            .inputs
            .iter()
            .filter_map(|input| {
                input.cell_entry.as_ref().map(|ue| {
                    (
                        CellEntryReference {
                            cell: Arc::new(ClientCellEntry::from_consensus_entry(None, input.previous_outpoint.into(), ue)),
                        },
                        ue.amount(),
                    )
                })
            })
            .fold((Vec::new(), 0), |(mut vec, sum), (entry, amount)| {
                vec.push(entry);
                (vec, sum + amount)
            });
        let first_output =
            inner_psst.outputs.first().cloned().ok_or_else(|| Error::Custom("0 outputs psst is not supported".to_string()))?;
        (cell_entries_ref, aggregate_input_value, first_output)
    };
    let signed_tx = match finalized_psst.extractor() {
        Ok(extractor) => match extractor.extract_tx(&network_id.into()) {
            Ok(tx) => tx.tx,
            Err(e) => return Err(Error::PendingTransactionFromPSSTError(e.to_string())),
        },
        Err(e) => return Err(Error::PendingTransactionFromPSSTError(e.to_string())),
    };
    if signed_tx.outputs.is_empty() {
        return Err(Error::Custom("0 outputs psst is not supported".to_string()));
        // todo support 0 outputs
    }
    let recipient = address_from_lock_script(&first_output.lock_script, network_id.into())?;
    let fee_u: u64 = 0;

    let cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static> =
        Box::new(cell_entries_ref.clone().into_iter());

    let final_transaction_destination = PaymentDestination::PaymentOutputs(PaymentOutputs::from((recipient, first_output.capacity)));

    let settings = GeneratorSettings {
        network_id,
        multiplexer: None,
        minimum_signatures: 1,
        change_address: change_address.clone(),
        cell_iterator,
        priority_cell_entries: None,
        source_cell_context,
        destination_cell_context: None,
        fee_rate: None,
        final_transaction_priority_fee: fee_u.into(),
        final_transaction_destination,
        final_transaction_payload: None,
        final_cellscript_compiled_scheduler_witness: None,
        cell_deps: signed_tx.cell_deps.clone(),
        header_deps: signed_tx.header_deps.clone(),
        ckb_type_id_output_indexes: Vec::new(),
        cellscript_typed_cell_scheduler_plan: None,
    };

    // Create the Generator
    let generator = Generator::try_new(settings, None, None)?;

    let aggregate_output_value = signed_tx.outputs.iter().map(|output| output.capacity).sum::<u64>();

    let (change_output_index, change_output_value) = signed_tx
        .outputs
        .iter()
        .enumerate()
        .find_map(|(idx, output)| {
            if let Ok(address) = address_from_lock_script(&output.lock, change_address.prefix) {
                if address == change_address {
                    Some((Some(idx), output.capacity))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or((None, 0));

    // Create PendingTransaction (WIP)
    let addresses = cell_entries_ref.iter().filter_map(|a| a.address()).collect();
    // todo where the source of mass and fees. why does it equal to zero?
    let pending_tx = PendingTransaction::try_new(
        &generator,
        signed_tx, // CellTx directly
        cell_entries_ref,
        addresses,
        Some(aggregate_output_value),
        change_output_index,
        change_output_value,
        aggregate_input_value,
        aggregate_output_value,
        1,
        0,
        0,
        DataKind::Final, // kind parameter
        None,
    )?;

    Ok(pending_tx)
}

fn address_from_lock_script(lock_script: &Script, prefix: Prefix) -> Result<Address, Error> {
    spora_consensus_core::tx::extract_address_from_script(lock_script, prefix)
        .map_err(|err| Error::Custom(format!("unsupported lock script for wallet address derivation: {err}")))
}

// Allow creation of atomic commit reveal operation with two
// different parameters sets.
pub enum CommitRevealBatchKind {
    Manual { hop_payment: PaymentDestination, destination_payment: PaymentDestination },
    Parameterized { address: Address, commit_amount_sau: u64 },
}

struct BundleCommitRevealConfig {
    pub address_commit: Address,
    pub addresses_reveal: Vec<Address>,
    pub commit_destination: PaymentDestination,
    pub witness_template: Vec<u8>,
    pub payment_outputs: PaymentOutputs,
}

// Create signed atomic commit reveal PSSB.
pub async fn commit_reveal_batch_bundle(
    batch_config: CommitRevealBatchKind,
    reveal_fee_sau: u64,
    witness_template: Vec<u8>,
    payload: Option<Vec<u8>>,
    fee_rate: Option<f64>,
    account: Arc<dyn Account>,
    wallet_secret: Secret,
    payment_secret: Option<Secret>,
    abortable: &Abortable,
) -> Result<Bundle, Error> {
    let network_id = account.wallet().clone().network_id()?;

    // Configure atomic batch of commit reveal transactions
    let conf: BundleCommitRevealConfig = match batch_config {
        CommitRevealBatchKind::Manual { hop_payment, destination_payment } => {
            let addr_commit = match hop_payment.clone() {
                PaymentDestination::Change => Err(Error::CommitRevealInvalidPaymentDestination),
                PaymentDestination::PaymentOutputs(payment_outputs) => {
                    payment_outputs.outputs.first().map(|out| out.address.clone()).ok_or(Error::CommitRevealEmptyPaymentOutputs)
                }
            }?;

            let (addresses, payment_outputs) = match destination_payment {
                PaymentDestination::Change => Err(Error::CommitRevealInvalidPaymentDestination),
                PaymentDestination::PaymentOutputs(payment_outputs) => {
                    Ok((payment_outputs.outputs.iter().map(|out| out.address.clone()).collect(), payment_outputs))
                }
            }?;

            BundleCommitRevealConfig {
                address_commit: addr_commit,
                addresses_reveal: addresses,
                commit_destination: hop_payment,
                witness_template: witness_template.clone(),
                payment_outputs,
            }
        }
        CommitRevealBatchKind::Parameterized { address, commit_amount_sau } => {
            let amt_reveal: u64 = commit_amount_sau - reveal_fee_sau;

            BundleCommitRevealConfig {
                address_commit: address.clone(),
                addresses_reveal: vec![address.clone()],
                commit_destination: PaymentDestination::from(PaymentOutput::new(address.clone(), commit_amount_sau)),
                witness_template: witness_template.clone(),
                payment_outputs: PaymentOutputs { outputs: vec![PaymentOutput::new(address.clone(), amt_reveal)] },
            }
        }
    };

    // Generate commit transaction
    let settings = GeneratorSettings::try_new_with_account(
        account.clone().as_dyn_arc(),
        conf.commit_destination.clone(),
        fee_rate.or(Some(1.0)),
        0u64.into(),
        payload,
    )
    .map_err(|e| Error::PSSTGenerationError(e.to_string()))?;

    let signer = Arc::new(PSSBSigner::new(
        account.clone().as_dyn_arc(),
        account.prv_key_data(wallet_secret.clone()).await?,
        payment_secret.clone(),
    ));

    let generator = Generator::try_new(settings, None, Some(abortable)).map_err(|e| Error::PSSTGenerationError(e.to_string()))?;

    let psst_generator = PSSTGenerator::new(generator, signer, account.wallet().address_prefix()?);

    let bundle_commit = bundle_from_psst_generator(psst_generator).await.map_err(|e| Error::PSSTGenerationError(e.to_string()))?;

    // Generate reveal transaction
    let bundle_unlock = unlock_cell_outputs_as_batch_transaction_pssb(
        conf.commit_destination.amount().unwrap(),
        &conf.address_commit,
        &conf.witness_template,
        conf.payment_outputs.outputs.into_iter().map(|i| (i.address.clone(), i.amount)).collect(),
    )
    .map_err(|e| Error::PSSTGenerationError(e.to_string()))?;

    // Sign and finalize commit transaction
    let (mut merge_bundle, commit_transaction_id) = {
        let signed_pssb = account
            .clone()
            .pssb_sign(&bundle_commit, wallet_secret.clone(), payment_secret.clone(), None)
            .await
            .map_err(|_| Error::CommitTransactionSigningError)?;

        let merge_bundle = Bundle::deserialize(&signed_pssb.serialize()?).map_err(|_| Error::CommitRevealBundleMergeError)?;

        let psst: PSST<Signer> = PSST::<Signer>::from(signed_pssb.as_ref()[0].to_owned());
        let finalizer = psst.finalizer();

        let psst_finalizer =
            finalize_psst_one_or_more_sig_and_witness_template(finalizer).map_err(|_| Error::PSSTFinalizationError)?;

        let transaction_id = psst_to_pending_transaction(
            psst_finalizer.clone(),
            network_id,
            account.change_address()?,
            account.cell_context().clone().into(),
        )
        .map_err(|_| Error::CommitTransactionIdExtractionError)?
        .id();
        (merge_bundle, transaction_id)
    };

    // Set commit transaction ID in reveal batch transaction input
    let reveal_psst: PSST<Signer> = PSST::<Signer>::from(bundle_unlock.as_ref()[0].to_owned());
    let unorphaned_bundle_unlock = Bundle::from(reveal_psst.set_input_prev_transaction_id(commit_transaction_id));

    // Try signing with each reveal address
    for reveal_address in &conf.addresses_reveal {
        if let Ok(signed_pssb) = account
            .clone()
            .pssb_sign(&unorphaned_bundle_unlock, wallet_secret.clone(), payment_secret.clone(), Some(reveal_address))
            .await
        {
            merge_bundle.merge(signed_pssb);
            return Ok(merge_bundle);
        }
    }

    Err(Error::NoQualifiedRevealSignerFound)
}
