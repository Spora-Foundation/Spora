use bitcoin::{taproot::Signature as BtcTaprootSignature, TapSighashType, Witness as BtcWitness};
use borsh::BorshDeserialize;
use secp256k1::{schnorr::Signature, Message, Secp256k1, XOnlyPublicKey};
// TxScriptError import removed as it's no longer used

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

// P2TrSpend is now private in official bitcoin crate
// This implementation is no longer needed as we use Witness methods directly

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

    pub fn into_inner(self) -> BtcWitness {
        self.inner
    }

    // Delegate methods to inner BtcWitness
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn last(&self) -> Option<&[u8]> {
        self.inner.last()
    }

    pub fn nth(&self, index: usize) -> Option<&[u8]> {
        self.inner.nth(index)
    }

    pub fn taproot_leaf_script(&self) -> Option<bitcoin::taproot::LeafScript<&bitcoin::Script>> {
        self.inner.taproot_leaf_script()
    }

    pub fn taproot_control_block(&self) -> Option<&[u8]> {
        self.inner.taproot_control_block()
    }
}
