use crate::imports::*;
use spora_wallet_core::account::pssb::finalize_psst_one_or_more_sig_and_witness_template;
use spora_wallet_psst::prelude::{Bundle, Signer, PSST};

#[derive(Default, Handler)]
#[help("Send a Spora transaction to a public address")]
pub struct Pssb;

impl Pssb {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;

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
                let amount_sau = try_parse_required_nonzero_spora_as_sau_u64(argv.get(1))?;
                let outputs = PaymentOutputs::from((address, amount_sau));
                let _priority_fee_sau = try_parse_optional_spora_as_sau_i64(argv.get(2))?.unwrap_or(0);
                let abortable = Abortable::default();

                let account: Arc<dyn Account> = ctx.wallet().account()?;
                let signer = account
                    .pssb_from_send_generator(
                        outputs.into(),
                        None,       // fee_rate
                        Fees::None, // _priority_fee_sau
                        None,       // payload
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
            "sign" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let pssb = Self::parse_input_pssb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pssb_sign(&pssb, wallet_secret.clone(), payment_secret.clone(), None).await {
                    Ok(signed_pssb) => {
                        let pssb_pack = String::try_from(signed_pssb)?;
                        tprintln!(ctx, "{pssb_pack}");
                    }
                    Err(e) => terrorln!(ctx, "{}", e.to_string()),
                }
            }
            "send" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pssb = Self::parse_input_pssb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pssb_broadcast(&pssb).await {
                    Ok(sent) => tprintln!(ctx, "Sent transactions {:?}", sent),
                    Err(e) => terrorln!(ctx, "Send error {:?}", e),
                }
            }
            "debug" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pssb = Self::parse_input_pssb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{:?}", pssb);
            }
            "parse" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pssb = Self::parse_input_pssb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{}", pssb.display_format(ctx.wallet().network_id()?, sau_to_spora_string_with_suffix));

                for (psst_index, bundle_inner) in pssb.0.iter().enumerate() {
                    tprintln!(ctx, "PSST #{:03} finalized check:", psst_index + 1);
                    let psst: PSST<Signer> = PSST::<Signer>::from(bundle_inner.to_owned());
                    let params = ctx.wallet().network_id()?.into();
                    let finalizer = psst.finalizer();

                    if let Ok(psst_finalizer) = finalize_psst_one_or_more_sig_and_witness_template(finalizer) {
                        // Verify if extraction is possible.
                        match psst_finalizer.extractor() {
                            Ok(ex) => match ex.extract_tx(&params) {
                                Ok(_) => tprintln!(
                                    ctx,
                                    "  Transaction extracted successfully: PSST is finalized with a valid script signature."
                                ),
                                Err(e) => terrorln!(ctx, "  PSST transaction extraction error: {}", e.to_string()),
                            },
                            Err(_) => twarnln!(ctx, "  PSST not finalized"),
                        }
                    } else {
                        twarnln!(ctx, "  PSST not signed");
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

    fn parse_input_pssb(input: &str) -> Result<Bundle> {
        match Bundle::try_from(input) {
            Ok(bundle) => Ok(bundle),
            Err(e) => Err(Error::custom(format!("Error while parsing input PSSB {}", e))),
        }
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<SporaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("pssb create <address> <amount> <priority fee>", "Create a PSSB from single send transaction"),
                ("pssb sign <pssb>", "Sign given PSSB"),
                ("pssb send <pssb>", "Broadcast bundled transactions"),
                ("pssb debug <payload>", "Print PSSB debug view"),
                ("pssb parse <payload>", "Print PSSB formatted view"),
            ],
            None,
        )?;

        Ok(())
    }
}
