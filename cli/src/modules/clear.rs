use crate::imports::*;
use workflow_terminal::clear::*;
use workflow_terminal::cursor::*;

#[derive(Default, Handler)]
#[help("Clear the terminal screen")]
pub struct Clear;

impl Clear {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<TondiCli>()?;
        
        // 使用workflow_terminal的内置清屏功能
        tprint!(ctx, "{}", ClearScreen);
        tprint!(ctx, "{}", Goto(1, 1));
        
        Ok(())
    }
}
