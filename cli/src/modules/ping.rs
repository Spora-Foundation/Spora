use crate::imports::*;

#[derive(Default, Handler)]
#[help("Ping the connected node")]
pub struct Ping;

impl Ping {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<TondiCli>()?;
        if ctx.wallet().ping(None).await.is_ok() {
            if ctx.pretty_enabled() {
                tprintln!(ctx, "✅ Ping successful - connection is working");
            } else {
                tprintln!(ctx, "Ping successful - connection is working");
            }
        } else {
            if ctx.pretty_enabled() {
                terrorln!(ctx, "❌ Ping failed - connection error");
            } else {
                terrorln!(ctx, "Ping failed - connection error");
            }
        }
        Ok(())
    }
}
