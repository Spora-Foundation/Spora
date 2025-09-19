use bitcoin::{taproot::Signature as BtcTaprootSignature, witness::P2TrSpend, TapSighashType, Witness as BtcWitness};
use borsh::BorshDeserialize;
use secp256k1::{schnorr::Signature, Message, Secp256k1, XOnlyPublicKey};
use tondi_txscript_errors::TxScriptError;

#[derive(Debug)]
pub struct Witness {
    inner: BtcWitness,
}

impl From<BtcWitness> for Witness {
    fn from(inner: BtcWitness) -> Self {
        Self { inner }
    }
}

impl TryFrom<&[u8]> for Witness {
    type Error = std::io::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let slice: Vec<Vec<u8>> = BorshDeserialize::try_from_slice(bytes)?;
        let inner = BtcWitness::from_slice(&slice);
        Ok(Self { inner })
    }
}

impl TryFrom<&Witness> for Vec<u8> {
    type Error = std::io::Error;

    fn try_from(witness: &Witness) -> Result<Self, Self::Error> {
        let slice = witness.inner.to_vec();
        borsh::to_vec(&slice)
    }
}

impl<'a> TryFrom<&'a Witness> for P2TrSpend<'a> {
    type Error = TxScriptError;

    fn try_from(witness: &'a Witness) -> Result<Self, Self::Error> {
        match P2TrSpend::from_witness(&witness.inner) {
            Some(p2tr) => Ok(p2tr),
            None => Err(TxScriptError::InvalidTaprootWitness),
        }
    }
}

impl Witness {
    pub fn p2tr_key_spend(signature: Signature, sighash_type: TapSighashType) -> Self {
        let taproot_signature = BtcTaprootSignature { signature, sighash_type };
        let inner = BtcWitness::p2tr_key_spend(&taproot_signature);
        Self { inner }
    }

    pub fn verify(&self, signature: &[u8], msg: &Message, xpub: &XOnlyPublicKey) -> Result<(), secp256k1::Error> {
        let secp = Secp256k1::new();
        let sig = Signature::from_slice(signature)?;
        secp.verify_schnorr(&sig, msg, xpub)
    }
}
