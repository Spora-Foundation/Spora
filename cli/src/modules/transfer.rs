use crate::imports::*;

#[derive(Default, Handler)]
#[help("Transfer funds between wallet accounts")]
pub struct Transfer;

impl Transfer {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<TondiCli>()?;

        let account = ctx.wallet().account()?;

        if argv.len() < 2 {
            tprintln!(ctx, "usage: transfer <account> <amount> <priority fee>");
            return Ok(());
        }

        let target_account = argv.first().unwrap();
        let target_account = ctx.find_accounts_by_name_or_id(target_account).await?;
        if target_account.id() == account.id() {
            return Err("Cannot transfer to the same account".into());
        }
        let amount_sau = try_parse_required_nonzero_tondi_as_sau_u64(argv.get(1))?;
        // TODO fee_rate
        let fee_rate: Option<f64> = None;
        let _priority_fee_sau = try_parse_optional_tondi_as_sau_i64(argv.get(2))?.unwrap_or(0);
        let target_address = target_account.receive_address()?;
        let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;

        let abortable = Abortable::default();
        let outputs = PaymentOutputs::from((target_address.clone(), amount_sau));

        // let ctx_ = ctx.clone();
        let (summary, _ids) = account
            .send(
                outputs.into(),
                fee_rate,
                _priority_fee_sau.into(),
                None,
                wallet_secret,
                payment_secret,
                &abortable,
                Some(Arc::new(move |_ptx| {
                    // tprintln!(ctx_, "Sending transaction: {}", ptx.id());
                })),
            )
            .await?;

        tprintln!(ctx, "Transfer - {summary}");

        Ok(())
    }
}
