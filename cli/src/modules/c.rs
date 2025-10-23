use crate::imports::*;
use workflow_terminal::clear::*;
use workflow_terminal::cursor::*;

#[derive(Default, Handler)]
#[help("Clear the terminal screen (alias for 'clear')")]
pub struct C;

impl C {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;

        // Execute clear screen functionality
        self.clear_screen(ctx).await?;
        Ok(())
    }

    async fn clear_screen(self: Arc<Self>, ctx: Arc<SporaCli>) -> Result<()> {
        // Use workflow_terminal's built-in clear screen functionality
        tprint!(ctx, "{}", ClearScreen);
        tprint!(ctx, "{}", Goto(1, 1));

        // Optional: Show a brief message that screen was cleared
        if ctx.pretty_enabled() {
            tprintln!(ctx, "✅ Screen cleared successfully");
        } else {
            tprintln!(ctx, "Screen cleared successfully");
        }

        Ok(())
    }
}
