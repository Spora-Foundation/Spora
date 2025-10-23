use crate::imports::*;

#[derive(Default, Handler)]
#[help("Toggle pretty display mode")]
pub struct Pretty;

impl Pretty {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;

        // Toggle pretty mode
        let current = ctx.pretty_enabled();
        let new_state = !current;
        ctx.set_pretty_enabled(new_state);

        // Show the new state
        if new_state {
            tprintln!(ctx, "✅ Pretty mode enabled - using emoji symbols");
        } else {
            tprintln!(ctx, "✅ Pretty mode disabled - using ASCII symbols");
        }

        ctx.term().refresh_prompt();
        Ok(())
    }
}
