use crate::api::traits::WalletApi;
use crate::imports::*;
use crate::result::Result;
use crate::storage::keydata;

/// @category Wallet SDK
#[wasm_bindgen]
pub struct PrvKeyDataInfo {
    inner: Arc<keydata::PrvKeyDataInfo>,
    #[allow(dead_code)]
    wallet: Arc<Wallet>,
}

impl PrvKeyDataInfo {
    pub fn new(wallet: Arc<Wallet>, inner: Arc<keydata::PrvKeyDataInfo>) -> PrvKeyDataInfo {
        PrvKeyDataInfo { wallet, inner }
    }
}

#[wasm_bindgen]
impl PrvKeyDataInfo {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.id.to_hex()
    }

    #[wasm_bindgen(getter)]
    pub fn name(&self) -> JsValue {
        self.inner.name.clone().map(JsValue::from).unwrap_or(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(getter, js_name = "isEncrypted")]
    pub fn is_encrypted(&self) -> JsValue {
        self.inner.is_encrypted.into()
    }

    #[wasm_bindgen(js_name = "setName")]
    pub async fn set_name(&mut self, name: String, wallet_secret: String) -> Result<()> {
        self.inner = self.wallet.clone().prv_key_data_rename(self.inner.id, Some(name), Secret::from(wallet_secret)).await?;

        Ok(())
    }
}
