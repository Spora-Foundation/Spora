//!
//! Structs used as various arguments for internal wallet operations.
//!

use crate::imports::*;
use crate::storage::interface::CreateArgs;
use crate::storage::{Hint, PrvKeyDataId};
use crate::wallet::keydata::PrvKeyDataVariantKind;
use borsh::{BorshDeserialize, BorshSerialize};
use zeroize::Zeroize;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateArgs {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub encryption_kind: EncryptionKind,
    pub user_hint: Option<Hint>,
    pub overwrite_wallet_storage: bool,
}

impl WalletCreateArgs {
    pub fn new(
        title: Option<String>,
        filename: Option<String>,
        encryption_kind: EncryptionKind,
        user_hint: Option<Hint>,
        overwrite_wallet_storage: bool,
    ) -> Self {
        Self { title, filename, encryption_kind, user_hint, overwrite_wallet_storage }
    }
}

impl From<WalletCreateArgs> for CreateArgs {
    fn from(args: WalletCreateArgs) -> Self {
        CreateArgs::new(args.title, args.filename, args.encryption_kind, args.user_hint, args.overwrite_wallet_storage)
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct WalletOpenArgs {
    /// Return account descriptors
    pub account_descriptors: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataCreateArgs {
    pub name: Option<String>,
    pub payment_secret: Option<Secret>,
    pub secret: Secret,
    pub kind: PrvKeyDataVariantKind,
}

impl PrvKeyDataCreateArgs {
    pub fn new(name: Option<String>, payment_secret: Option<Secret>, secret: Secret, kind: PrvKeyDataVariantKind) -> Self {
        Self { name, payment_secret, secret, kind }
    }
}

impl Zeroize for PrvKeyDataCreateArgs {
    fn zeroize(&mut self) {
        self.secret.zeroize();
        self.payment_secret.zeroize();
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_ACCOUNT_CREATE_ARGS: &'static str = r#"

export interface IPrvKeyDataArgs {
    prvKeyDataId: HexString;
    paymentSecret?: string;
}

export interface IAccountCreateArgsBip32 {
    accountName?: string;
    accountIndex?: number;
}

export interface IAccountCreateArgsWatchOnly {
    accountName?: string;
    xpubKeys: string[];
    minimumSignatures?: number;
    ecdsa?: boolean;
}

export interface IAccountCreateArgsMultisig {
    accountName?: string;
    prvKeyDataIds?: HexString[];
    additionalXpubKeys?: string[];
    minimumSignatures?: number;
    paymentSecret?: string;
}

export interface IAccountCreateArgsKeypair {
    accountName?: string;
    prvKeyDataId: HexString;
    ecdsa?: boolean;
}

export interface IAccountCreateArgsBip32Watch {
    accountName?: string;
    xpubKeys: string[];
}

/**
 * @category Wallet API
 */
export type IAccountCreateArgs =
    | {
        type : "bip32";
        args : IAccountCreateArgsBip32;
        prvKeyDataArgs? : IPrvKeyDataArgs;
      }
    | {
        type : "keypair";
        args : IAccountCreateArgsKeypair;
      }
    | {
        type : "bip32watch";
        args : IAccountCreateArgsBip32Watch;
      }
    | {
        type : "watchonly";
        args : IAccountCreateArgsWatchOnly;
      }
    | {
        type : "multisig";
        args : IAccountCreateArgsMultisig;
      };
"#;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountCreateArgsBip32 {
    pub account_name: Option<String>,
    pub account_index: Option<u64>,
}

impl AccountCreateArgsBip32 {
    pub fn new(account_name: Option<String>, account_index: Option<u64>) -> Self {
        Self { account_name, account_index }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountCreateArgsBip32Watch {
    pub account_name: Option<String>,
    pub xpub_keys: Vec<String>,
}

impl AccountCreateArgsBip32Watch {
    pub fn new(account_name: Option<String>, xpub_keys: Vec<String>) -> Self {
        Self { account_name, xpub_keys }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountCreateArgsWatchOnly {
    pub account_name: Option<String>,
    pub xpub_keys: Vec<String>,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

impl AccountCreateArgsWatchOnly {
    pub fn new(account_name: Option<String>, xpub_keys: Vec<String>, minimum_signatures: u16, ecdsa: bool) -> Self {
        Self { account_name, xpub_keys, minimum_signatures, ecdsa }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataArgs {
    pub prv_key_data_id: PrvKeyDataId,
    pub payment_secret: Option<Secret>,
}

impl PrvKeyDataArgs {
    pub fn new(prv_key_data_id: PrvKeyDataId, payment_secret: Option<Secret>) -> Self {
        Self { prv_key_data_id, payment_secret }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "args")]
pub enum AccountCreateArgs {
    Bip32 {
        prv_key_data_args: PrvKeyDataArgs,
        account_args: AccountCreateArgsBip32,
    },
    Multisig {
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        additional_xpub_keys: Vec<String>,
        name: Option<String>,
        minimum_signatures: u16,
    },
    Bip32Watch {
        account_args: AccountCreateArgsBip32Watch,
    },
    WatchOnly {
        account_args: AccountCreateArgsWatchOnly,
    },
    Keypair {
        prv_key_data_id: PrvKeyDataId,
        account_name: Option<String>,
        ecdsa: bool,
    },
}
impl AccountCreateArgs {
    pub fn new_bip32(
        prv_key_data_id: PrvKeyDataId,
        payment_secret: Option<Secret>,
        account_name: Option<String>,
        account_index: Option<u64>,
    ) -> Self {
        let prv_key_data_args = PrvKeyDataArgs { prv_key_data_id, payment_secret };
        let account_args = AccountCreateArgsBip32 { account_name, account_index };
        AccountCreateArgs::Bip32 { prv_key_data_args, account_args }
    }

    pub fn new_keypair_key(prv_key_data_id: PrvKeyDataId, account_name: Option<String>, ecdsa: bool) -> Self {
        AccountCreateArgs::Keypair { prv_key_data_id, account_name, ecdsa }
    }

    pub fn new_watch_only(account_name: Option<String>, xpub_keys: Vec<String>, minimum_signatures: u16, ecdsa: bool) -> Self {
        let account_args = AccountCreateArgsWatchOnly { account_name, xpub_keys, minimum_signatures, ecdsa };
        AccountCreateArgs::WatchOnly { account_args }
    }

    pub fn new_multisig(
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        additional_xpub_keys: Vec<String>,
        name: Option<String>,
        minimum_signatures: u16,
    ) -> Self {
        AccountCreateArgs::Multisig { prv_key_data_args, additional_xpub_keys, name, minimum_signatures }
    }
}
