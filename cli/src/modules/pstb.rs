#![allow(unused_imports)]

use crate::imports::*;
use tondi_addresses::Prefix;
use tondi_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use tondi_wallet_core::account::pstb::finalize_pstt_one_or_more_sig_and_redeem_script;
use tondi_wallet_pstt::{
    prelude::{lock_script_sig_templating, script_sig_to_address, unlock_utxos_as_pstb, Bundle, Signer, PSTT},
    pstt::Inner,
};

#[derive(Default, Handler)]
#[help("Send a Tondi transaction to a public address")]
pub struct Pstb;

impl Pstb {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<TondiCli>()?;

        if !ctx.wallet().is_open() {
            return Err(Error::WalletIsNotOpen);
        }

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        let action = argv.remove(0);

        match action.as_str() {
            "create" => {
                if argv.len() < 2 || argv.len() > 3 {
                    return self.display_help(ctx, argv).await;
                }
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let _ = ctx.notifier().show(Notification::Processing).await;

                let address = Address::try_from(argv.first().unwrap().as_str())?;
                let amount_sau = try_parse_required_nonzero_tondi_as_sau_u64(argv.get(1))?;
                let outputs = PaymentOutputs::from((address, amount_sau));
                let _priority_fee_sau = try_parse_optional_tondi_as_sau_i64(argv.get(2))?.unwrap_or(0);
                let abortable = Abortable::default();

                let account: Arc<dyn Account> = ctx.wallet().account()?;
                let signer = account
                    .pstb_from_send_generator(
                        outputs.into(),
                        None, // fee_rate
                        Fees::None, // _priority_fee_sau
                        None, // payload
                        wallet_secret.clone(),
                        payment_secret.clone(),
                        &abortable,
                    )
                    .await?;

                match signer.serialize() {
                    Ok(encoded) => tprintln!(ctx, "{encoded}"),
                    Err(e) => return Err(e.into()),
                }
            }
            "script" => {
                if argv.len() < 2 || argv.len() > 4 {
                    return self.display_help(ctx, argv).await;
                }
                let subcommand = argv.remove(0);
                let payload = argv.remove(0);
                let account = ctx.wallet().account()?;
                let receive_address = account.receive_address()?;
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let _ = ctx.notifier().show(Notification::Processing).await;

                let script_sig = match lock_script_sig_templating(payload.clone(), Some(&receive_address.payload)) {
                    Ok(value) => value,
                    Err(e) => {
                        terrorln!(ctx, "{}", e.to_string());
                        return Err(e.into());
                    }
                };

                let script_p2sh = match script_sig_to_address(&script_sig, ctx.wallet().address_prefix()?) {
                    Ok(p2sh) => p2sh,
                    Err(e) => {
                        terrorln!(ctx, "Error generating script address: {}", e.to_string());
                        return Err(e.into());
                    }
                };

                match subcommand.as_str() {
                    "lock" => {
                        let amount_sau = try_parse_required_nonzero_tondi_as_sau_u64(argv.first())?;
                        let outputs = PaymentOutputs::from((script_p2sh, amount_sau));
                        let fee_rate = None;
                        let _priority_fee_sau = try_parse_optional_tondi_as_sau_i64(argv.get(1))?.unwrap_or(0);
                        let abortable = Abortable::default();

                        let signer = account
                            .pstb_from_send_generator(
                                outputs.into(),
                                fee_rate,
                                _priority_fee_sau.into(),
                                None,
                                wallet_secret.clone(),
                                payment_secret.clone(),
                                &abortable,
                            )
                            .await?;

                        match signer.serialize() {
                            Ok(encoded) => tprintln!(ctx, "{encoded}"),
                            Err(e) => return Err(e.into()),
                        }
                    }
                    "unlock" => {
                        if argv.len() != 1 {
                            return self.display_help(ctx, argv).await;
                        }

                        // Get locked UTXO set.
                        let spend_utxos: Vec<tondi_rpc_core::RpcUtxosByAddressesEntry> =
                            ctx.wallet().rpc_api().get_utxos_by_addresses(vec![script_p2sh.clone()]).await?;
                        let _priority_fee_sau = try_parse_optional_tondi_as_sau_i64(argv.first())?.unwrap_or(0) as u64;

                        if spend_utxos.is_empty() {
                            twarnln!(ctx, "No locked UTXO set found.");
                            return Ok(());
                        }

                        let references: Vec<(UtxoEntry, TransactionOutpoint)> =
                            spend_utxos.iter().map(|entry| (entry.utxo_entry.clone().into(), entry.outpoint.into())).collect();

                        let total_locked_sau: u64 = spend_utxos.iter().map(|entry| entry.utxo_entry.amount).sum();

                        tprintln!(
                            ctx,
                            "{} locked UTXO{} found with total amount of {} TONDI",
                            spend_utxos.len(),
                            if spend_utxos.len() == 1 { "" } else { "s" },
                            sau_to_tondi(total_locked_sau)
                        );

                        // Sweep UTXO set.
                        match unlock_utxos_as_pstb(references, &receive_address, script_sig, _priority_fee_sau as u64) {
                            Ok(pstb) => {
                                let pstb_hex = pstb.serialize()?;
                                tprintln!(ctx, "{pstb_hex}");
                            }
                            Err(e) => tprintln!(ctx, "Error generating unlock PSTB: {}", e.to_string()),
                        }
                    }
                    "sign" => {
                        let pstb = Self::parse_input_pstb(argv.first().unwrap().as_str())?;

                        // Sign PSTB using the account's receiver address.
                        match account.pstb_sign(&pstb, wallet_secret.clone(), payment_secret.clone(), Some(&receive_address)).await {
                            Ok(signed_pstb) => {
                                let pstb_pack = String::try_from(signed_pstb)?;
                                tprintln!(ctx, "{pstb_pack}");
                            }
                            Err(e) => terrorln!(ctx, "{}", e.to_string()),
                        }
                    }
                    "address" => {
                        tprintln!(ctx, "\r\nP2SH address: {}", script_p2sh);
                    }
                    v => {
                        terrorln!(ctx, "unknown command: '{v}'\r\n");
                        return self.display_help(ctx, argv).await;
                    }
                }
            }
            "sign" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let pstb = Self::parse_input_pstb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pstb_sign(&pstb, wallet_secret.clone(), payment_secret.clone(), None).await {
                    Ok(signed_pstb) => {
                        let pstb_pack = String::try_from(signed_pstb)?;
                        tprintln!(ctx, "{pstb_pack}");
                    }
                    Err(e) => terrorln!(ctx, "{}", e.to_string()),
                }
            }
            "send" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pstb = Self::parse_input_pstb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pstb_broadcast(&pstb).await {
                    Ok(sent) => tprintln!(ctx, "Sent transactions {:?}", sent),
                    Err(e) => terrorln!(ctx, "Send error {:?}", e),
                }
            }
            "debug" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pstb = Self::parse_input_pstb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{:?}", pstb);
            }
            "parse" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pstb = Self::parse_input_pstb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{}", pstb.display_format(ctx.wallet().network_id()?, sau_to_tondi_string_with_suffix));

                for (pstt_index, bundle_inner) in pstb.0.iter().enumerate() {
                    tprintln!(ctx, "PSTT #{:03} finalized check:", pstt_index + 1);
                    let pstt: PSTT<Signer> = PSTT::<Signer>::from(bundle_inner.to_owned());
                    let params = ctx.wallet().network_id()?.into();
                    let finalizer = pstt.finalizer();

                    if let Ok(pstt_finalizer) = finalize_pstt_one_or_more_sig_and_redeem_script(finalizer) {
                        // Verify if extraction is possible.
                        match pstt_finalizer.extractor() {
                            Ok(ex) => match ex.extract_tx(&params) {
                                Ok(_) => tprintln!(
                                    ctx,
                                    "  Transaction extracted successfully: PSTT is finalized with a valid script signature."
                                ),
                                Err(e) => terrorln!(ctx, "  PSTT transaction extraction error: {}", e.to_string()),
                            },
                            Err(_) => twarnln!(ctx, "  PSTT not finalized"),
                        }
                    } else {
                        twarnln!(ctx, "  PSTT not signed");
                    }
                }
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }
        Ok(())
    }

    fn parse_input_pstb(input: &str) -> Result<Bundle> {
        match Bundle::try_from(input) {
            Ok(bundle) => Ok(bundle),
            Err(e) => Err(Error::custom(format!("Error while parsing input PSTB {}", e))),
        }
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<TondiCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("pstb create <address> <amount> <priority fee>", "Create a PSTB from single send transaction"),
                ("pstb sign <pstb>", "Sign given PSTB"),
                ("pstb send <pstb>", "Broadcast bundled transactions"),
                ("pstb debug <payload>", "Print PSTB debug view"),
                ("pstb parse <payload>", "Print PSTB formatted view"),
                ("pstb script lock <payload> <amount> [priority fee]", "Generate a PSTB with one send transaction to given P2SH payload. Optional public key placeholder in payload: {{pubkey}}"),
                ("pstb script unlock <payload> <fee>", "Generate a PSTB to unlock UTXOS one by one from given P2SH payload. Fee amount will be applied to every spent UTXO, meaning every transaction. Optional public key placeholder in payload: {{pubkey}}"),
                ("pstb script sign <pstb>", "Sign all PSTB's P2SH locked inputs"),
                ("pstb script sign <pstb>", "Sign all PSTB's P2SH locked inputs"),
                ("pstb script address <pstb>", "Prints P2SH address"),
            ],
            None,
        )?;

        Ok(())
    }
}
