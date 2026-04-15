//!
//! WASM bindings for transaction hashers: [`CellTxSigningHash`](native::CellTxSigningHash)
//! and [`CellTxSigningHashEcdsa`](native::CellTxSigningHashEcdsa).
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;
use spora_hashes as native;
use spora_hashes::HasherBase;
use spora_wasm_core::types::BinaryT;

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct CellTxSigningHash {
    hasher: native::CellTxSigningHash,
}

#[wasm_bindgen]
impl CellTxSigningHash {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::CellTxSigningHash::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct CellTxSigningHashEcdsa {
    hasher: native::CellTxSigningHashEcdsa,
}

#[wasm_bindgen]
impl CellTxSigningHashEcdsa {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::CellTxSigningHashEcdsa::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}
