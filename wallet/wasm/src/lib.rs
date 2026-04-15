//! # Spora Wallet WASM CLI
//!
//! This crate provides a WebAssembly (WASM) entry point for the Spora wallet
//! command-line interface.  It is designed to be loaded inside a browser or
//! any other WASM host that supports the `wasm_bindgen` ABI.
//!
//! ## Usage (JavaScript / TypeScript)
//!
//! ```js
//! import { load_spora_wallet_cli } from 'spora-wallet-wasm';
//! await load_spora_wallet_cli();
//! ```

use spora_cli_lib::spora_cli;
use wasm_bindgen::prelude::*;
use workflow_terminal::Options;
use workflow_terminal::Result;

/// Bootstrap and run the interactive Spora wallet CLI inside a WASM
/// environment.
///
/// This function initialises a [`workflow_terminal`] session with default
/// options and delegates to [`spora_cli`](spora_cli_lib::spora_cli) for
/// the main command loop.
///
/// # Errors
///
/// Returns an error if the terminal or the CLI fails to initialise.
#[wasm_bindgen]
pub async fn load_spora_wallet_cli() -> Result<()> {
    let options = Options { ..Options::default() };
    spora_cli(options, None).await?;
    Ok(())
}
