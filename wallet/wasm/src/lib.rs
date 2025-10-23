use spora_cli_lib::spora_cli;
use wasm_bindgen::prelude::*;
use workflow_terminal::Options;
use workflow_terminal::Result;

#[wasm_bindgen]
pub async fn load_tondi_wallet_cli() -> Result<()> {
    let options = Options { ..Options::default() };
    spora_cli(options, None).await?;
    Ok(())
}
