use crate::imports::*;
use workflow_terminal::clear::*;
use workflow_terminal::cursor::*;

#[derive(Default, Handler)]
#[help("Clear the terminal screen")]
pub struct Clear;

impl Clear {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;

        // Check for clear command
        if argv.is_empty() || argv.iter().any(|arg| arg.to_lowercase() == "clear") {
            self.clear_screen(ctx).await?;
            return Ok(());
        }

        // Show help if unknown arguments
        self.show_help(ctx).await
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

    async fn show_help(self: Arc<Self>, ctx: Arc<SporaCli>) -> Result<()> {
        tprintln!(ctx, "Clear screen commands:");
        tprintln!(ctx, "  clear                     - Clear the terminal screen");
        tprintln!(ctx, "  c                         - Short clear command (alias)");
        tprintln!(ctx, "");
        tprintln!(ctx, "The clear command will:");
        tprintln!(ctx, "  • Clear all text from the terminal");
        tprintln!(ctx, "  • Move cursor to top-left position");
        tprintln!(ctx, "  • Provide a clean workspace");
        Ok(())
    }
}
