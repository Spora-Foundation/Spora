//!
//! Transaction signing trait and generic signer implementations..
//!

use crate::imports::*;
use spora_bip32::PrivateKey;
use spora_consensus_core::{sign::sign_with_multiple_v2, tx::SignableTransaction};

pub trait SignerT: Send + Sync + 'static {
    fn try_sign(&self, transaction: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction>;
}

struct Inner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    keys: Mutex<AHashMap<Address, [u8; 32]>>,
}

pub struct Signer {
    inner: Arc<Inner>,
}

impl Signer {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self { inner: Arc::new(Inner { keydata, account, payment_secret, keys: Mutex::new(AHashMap::new()) }) }
    }

    fn ingest(&self, addresses: &[Address]) -> Result<()> {
        let mut keys = self.inner.keys.lock().unwrap();
        // skip address that are already present in the key map
        let addresses = addresses.iter().filter(|a| !keys.contains_key(a)).collect::<Vec<_>>();
        if !addresses.is_empty() {
            // let account = self.inner.account.clone().as_derivation_capable().expect("expecting derivation capable account");
            // let (receive, change) = account.derivation().addresses_indexes(&addresses)?;
            // let private_keys = account.create_private_keys(&self.inner.keydata, &self.inner.payment_secret, &receive, &change)?;
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
}

impl SignerT for Signer {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        self.ingest(addresses)?;

        let keys = self.inner.keys.lock().unwrap();
        let mut keys_for_signing = addresses.iter().filter_map(|address| keys.get(address).copied()).collect::<Vec<_>>();
        if keys_for_signing.is_empty() {
            return Err(Error::custom("no signing keys available for supplied addresses"));
        }
        let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).unwrap();
        keys_for_signing.zeroize();
        Ok(signable_tx)
    }
}

// ---

struct KeydataSignerInner {
    keys: HashMap<Address, [u8; 32]>,
}

pub struct KeydataSigner {
    inner: Arc<KeydataSignerInner>,
}

impl KeydataSigner {
    pub fn new(keydata: Vec<(Address, secp256k1::SecretKey)>) -> Self {
        let keys = keydata.into_iter().map(|(address, key)| (address, key.to_bytes())).collect();
        Self { inner: Arc::new(KeydataSignerInner { keys }) }
    }
}

impl SignerT for KeydataSigner {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        let mut keys_for_signing = addresses.iter().filter_map(|address| self.inner.keys.get(address).copied()).collect::<Vec<_>>();
        if keys_for_signing.is_empty() {
            return Err(Error::custom("no signing keys available for supplied addresses"));
        }
        let signable_tx = sign_with_multiple_v2(mutable_tx, &keys_for_signing).unwrap();
        keys_for_signing.zeroize();
        Ok(signable_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::{KeydataSigner, SignerT};
    use secp256k1::Secp256k1;
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::{
        cell_diff::CellMeta,
        cell_metadata::CellMetadata,
        sign::verify,
        tx::{pay_to_address_lock_script, MutableTransaction, TransactionOutpoint},
    };
    use spora_exec::{CellInput, CellOutput, CellTx};

    #[test]
    fn keydata_signer_signs_pubkey_ecdsa_transactions() {
        let secp = Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(&[0x31; 32]).expect("valid secret key");
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let address = Address::new_std_single_ecdsa(Prefix::Testnet, &public_key.serialize()).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(TransactionOutpoint::new([0x41; 32], 0), 0)],
            vec![],
            vec![CellOutput { capacity: 100, lock: lock_script.clone(), type_: None }],
            vec![vec![]],
            vec![vec![]],
        )
        .expect("valid tx");
        let metadata = CellMetadata {
            out_point: TransactionOutpoint::new([0x41; 32], 0),
            capacity: 200,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: TransactionOutpoint::new([0x42; 32], 0).tx_hash.into(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let signer = KeydataSigner::new(vec![(address.clone(), secret_key)]);
        let mut signed_tx = signer
            .try_sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![metadata]), &[address])
            .expect("ecdsa signing succeeds");
        signed_tx.entries[0] = Some(CellMeta {
            out_point: TransactionOutpoint::new([0x41; 32], 0),
            capacity: 200,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
        });

        assert!(verify(&signed_tx.as_verifiable()).is_ok());
        assert_eq!(signed_tx.tx.witnesses.len(), 1);
        assert_eq!(signed_tx.tx.witnesses[0].first().copied(), Some(1));
    }

    #[test]
    fn keydata_signer_returns_partially_signed_transaction_when_some_keys_are_missing() {
        let secp = Secp256k1::new();
        let secret_key_a = secp256k1::SecretKey::from_slice(&[0x31; 32]).expect("valid secret key");
        let public_key_a = secp256k1::PublicKey::from_secret_key(&secp, &secret_key_a);
        let address_a = Address::new_std_single_ecdsa(Prefix::Testnet, &public_key_a.serialize()).expect("valid address");
        let lock_script_a = pay_to_address_lock_script(&address_a);

        let secret_key_b = secp256k1::SecretKey::from_slice(&[0x32; 32]).expect("valid secret key");
        let public_key_b = secp256k1::PublicKey::from_secret_key(&secp, &secret_key_b);
        let address_b = Address::new_std_single_ecdsa(Prefix::Testnet, &public_key_b.serialize()).expect("valid address");
        let lock_script_b = pay_to_address_lock_script(&address_b);

        let unsigned_tx = CellTx::new(
            vec![
                CellInput::new(TransactionOutpoint::new([0x41; 32], 0), 0),
                CellInput::new(TransactionOutpoint::new([0x42; 32], 0), 0),
            ],
            vec![],
            vec![CellOutput { capacity: 200, lock: lock_script_a.clone(), type_: None }],
            vec![vec![]],
            vec![vec![], vec![]],
        )
        .expect("valid tx");

        let metadata = vec![
            CellMetadata {
                out_point: TransactionOutpoint::new([0x41; 32], 0),
                capacity: 100,
                data_bytes: 0,
                lock_hash: lock_script_a.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: TransactionOutpoint::new([0x51; 32], 0).tx_hash.into(),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(lock_script_a.clone()),
                type_script: None,
                data: Some(vec![]),
            },
            CellMetadata {
                out_point: TransactionOutpoint::new([0x42; 32], 0),
                capacity: 100,
                data_bytes: 0,
                lock_hash: lock_script_b.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: TransactionOutpoint::new([0x52; 32], 0).tx_hash.into(),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(lock_script_b.clone()),
                type_script: None,
                data: Some(vec![]),
            },
        ];

        let signer = KeydataSigner::new(vec![(address_a.clone(), secret_key_a)]);
        let signed_tx = signer
            .try_sign(MutableTransaction::with_resolved_metadata(unsigned_tx, metadata), &[address_a.clone(), address_b.clone()])
            .expect("partial signing should succeed");

        assert_eq!(signed_tx.tx.witnesses.len(), 2);
        assert!(!signed_tx.tx.witnesses[0].is_empty(), "input with available key should be signed");
        assert!(signed_tx.tx.witnesses[1].is_empty(), "input without a key should remain unsigned");
    }
}
