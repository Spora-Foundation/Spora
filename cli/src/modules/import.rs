use crate::imports::*;

#[derive(Default, Handler)]
#[help("Import a wallet, mnemonic, or a private key")]
pub struct Import;

impl Import {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;
        let wallet = ctx.wallet();

        if argv.is_empty() {
            self.display_help(ctx).await?;
            return Ok(());
        }

        let what = argv.get(0).unwrap();
        match what.as_str() {
            "mnemonic" => {
                let account_kind =
                    if let Some(account_kind) = argv.get(1) { account_kind.parse::<AccountKind>()? } else { AccountKind::Bip32 };
                if argv.len() > 1 {
                    crate::wizards::import::import_with_mnemonic(&ctx, account_kind, &argv[2..]).await?;
                } else {
                    crate::wizards::import::import_with_mnemonic(&ctx, account_kind, &[]).await?;
                }
            }
            // todo "read-only" => {}
            // "core" => {}
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<SporaCli>) -> Result<()> {
        ctx.term().help(
            &[
                (
                    "mnemonic [<type>] [<additional xpub keys>] ",
                    "Import a mnemonic (types: 'bip32' (default), 'multisig').",
                ),
                // ("purge", "Purge an account from the wallet"),
            ],
            None,
        )?;

        Ok(())
    }
}
