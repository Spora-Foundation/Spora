use crate::imports::*;
use crate::result::Result;
use js_sys::Array;
use spora_consensus_client::{sign_with_multiple, Transaction};
use spora_consensus_core::hashing::wasm::SighashType;
use spora_consensus_core::sign::sign_input;
use spora_consensus_core::sign::verify;
use spora_wallet_keys::privatekey::PrivateKey;
use spora_wasm_core::types::HexString;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "(PrivateKey | HexString | Uint8Array)[]")]
    #[derive(Clone, Debug, PartialEq)]
    pub type PrivateKeyArrayT;
}

impl TryFrom<PrivateKeyArrayT> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArrayT) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys
                .push(PrivateKey::try_owned_from(key).map_err(|_| Self::Error::Custom("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}

/// `signTransaction()` is a helper function to sign a transaction using a private key array or a signer array.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "signTransaction")]
pub fn js_sign_transaction(tx: &Transaction, signer: &PrivateKeyArrayT, verify_sig: bool) -> Result<Transaction> {
    if signer.is_array() {
        let mut private_keys: Vec<[u8; 32]> = vec![];
        for key in Array::from(signer).iter() {
            let key = PrivateKey::try_cast_from(&key).map_err(|_| Error::Custom("Unable to cast PrivateKey".to_string()))?;
            private_keys.push(key.as_ref().secret_bytes());
        }

        let tx = sign_transaction(tx, &private_keys, verify_sig).map_err(|err| Error::Custom(format!("Unable to sign: {err:?}")))?;
        private_keys.zeroize();
        Ok(tx.clone())
    } else {
        Err(Error::custom("signTransaction() requires an array of signatures"))
    }
}

fn sign_transaction<'a>(tx: &'a Transaction, private_keys: &[[u8; 32]], verify_sig: bool) -> Result<&'a Transaction> {
    let tx = sign(tx, private_keys)?;
    if verify_sig {
        let signable_tx = tx.signable_transaction()?;
        let verifiable_tx = signable_tx.as_verifiable();
        verify(&verifiable_tx)?;
    }
    Ok(tx)
}

/// Sign a transaction using schnorr, returns a new transaction with the signatures added.
/// The resulting transaction may be partially signed if the supplied keys are not sufficient
/// to sign all of its inputs.
pub fn sign<'a>(tx: &'a Transaction, privkeys: &[[u8; 32]]) -> Result<&'a Transaction> {
    Ok(sign_with_multiple(tx, privkeys)?.unwrap())
}

/// `createInputSignature()` is a helper function to sign a transaction input with a specific SigHash type using a private key.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "createInputSignature")]
pub fn create_input_signature(
    tx: &Transaction,
    input_index: u8,
    private_key: &PrivateKey,
    sighash_type: Option<SighashType>,
) -> Result<HexString> {
    let signable_tx = tx.signable_transaction()?;
    let verifiable_tx = signable_tx.as_verifiable();

    let signature =
        sign_input(&verifiable_tx, input_index.into(), &private_key.secret_bytes(), sighash_type.unwrap_or(SighashType::All).into());

    Ok(signature.to_hex().into())
}
