//!
//! Id references used to associate transactions with Account or CellContext ids.
//!

use crate::cell::{CellContextBinding, CellContextId};
use crate::imports::*;

#[wasm_bindgen(typescript_custom_section)]
const ITransactionRecord: &'static str = r#"

/**
 * Type of a binding record.
 * @see {@link IBinding}, {@link ITransactionDataVariant}, {@link ITransactionRecord}
 * @category Wallet SDK
 */
export enum BindingType {
    /**
     * The data structure is associated with a user-supplied id.
     * @see {@link IBinding}
     */
    Custom = "custom",
    /**
     * The data structure is associated with a wallet account.
     * @see {@link IBinding}, {@link Account}
     */
    Account = "account",
}

/**
 * Internal transaction data contained within the transaction record.
 * @see {@link ITransactionRecord}
 * @category Wallet SDK
 */
export interface IBinding {
    type : BindingType;
    id : HexString;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Object, typescript_type = "IBinding")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type BindingT;
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "id")]
pub enum Binding {
    Custom(CellContextId),
    Account(AccountId),
}

impl From<CellContextBinding> for Binding {
    fn from(b: CellContextBinding) -> Self {
        match b {
            CellContextBinding::Internal(id) => Binding::Custom(id),
            CellContextBinding::Id(id) => Binding::Custom(id),
            CellContextBinding::AccountId(id) => Binding::Account(id),
        }
    }
}

impl From<&Arc<dyn Account>> for Binding {
    fn from(account: &Arc<dyn Account>) -> Self {
        Binding::Account(*account.id())
    }
}

impl Binding {
    pub fn to_hex(&self) -> String {
        match self {
            Binding::Custom(id) => id.to_hex(),
            Binding::Account(id) => id.to_hex(),
        }
    }
}

impl AsRef<Binding> for Binding {
    fn as_ref(&self) -> &Binding {
        self
    }
}
