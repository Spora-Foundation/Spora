//! Copperoot witness implementation
//!
//! This module implements witness structures for Copperoot transactions,
//! supporting both key path and script path spending with Verkle proof support.

use bitcoin::{taproot::Signature as BtcTaprootSignature, witness::P2TrSpend, TapSighashType, Witness as BtcWitness};
use borsh::BorshDeserialize;
use secp256k1::{schnorr::Signature, Message, Secp256k1, XOnlyPublicKey};
use tondi_txscript_errors::TxScriptError;

use crate::standard::copperoot::verkle::VerkleProof;

/// Copperoot witness structure
#[derive(Debug, Clone)]
pub struct CopperootWitness {
    /// Inner Bitcoin witness
    inner: BtcWitness,
    /// Verkle proof (if present)
    verkle_proof: Option<VerkleProof>,
}

impl From<BtcWitness> for CopperootWitness {
    fn from(inner: BtcWitness) -> Self {
        Self { inner, verkle_proof: None }
    }
}

impl TryFrom<&[u8]> for CopperootWitness {
    type Error = std::io::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let slice: Vec<Vec<u8>> = BorshDeserialize::try_from_slice(bytes)?;
        let inner = BtcWitness::from_slice(&slice);
        Ok(Self { inner, verkle_proof: None })
    }
}

impl TryFrom<&CopperootWitness> for Vec<u8> {
    type Error = std::io::Error;

    fn try_from(witness: &CopperootWitness) -> Result<Self, Self::Error> {
        let slice = witness.inner.to_vec();
        borsh::to_vec(&slice)
    }
}

impl<'a> TryFrom<&'a CopperootWitness> for P2TrSpend<'a> {
    type Error = TxScriptError;

    fn try_from(witness: &'a CopperootWitness) -> Result<Self, Self::Error> {
        match P2TrSpend::from_witness(&witness.inner) {
            Some(p2tr) => Ok(p2tr),
            None => Err(TxScriptError::InvalidTaprootWitness),
        }
    }
}

impl CopperootWitness {
    /// Create a key path spend witness
    pub fn p2tr_key_spend(signature: Signature, sighash_type: TapSighashType) -> Self {
        let taproot_signature = BtcTaprootSignature { signature, sighash_type };
        let inner = BtcWitness::p2tr_key_spend(&taproot_signature);
        Self { inner, verkle_proof: None }
    }

    /// Create a script path spend witness with Verkle proof
    pub fn p2tr_script_spend_with_verkle(
        script: Vec<u8>,
        control_block: Vec<u8>,
        verkle_proof: VerkleProof,
    ) -> Self {
        let mut inner = BtcWitness::new();
        inner.push(script);
        inner.push(control_block);
        Self { inner, verkle_proof: Some(verkle_proof) }
    }

    /// Create a script path spend witness without Verkle proof
    pub fn p2tr_script_spend(script: Vec<u8>, control_block: Vec<u8>) -> Self {
        let mut inner = BtcWitness::new();
        inner.push(script);
        inner.push(control_block);
        Self { inner, verkle_proof: None }
    }

    /// Verify the witness
    pub fn verify(&self, signature: &[u8], msg: &Message, xpub: &XOnlyPublicKey) -> Result<(), secp256k1::Error> {
        let secp = Secp256k1::new();
        let sig = Signature::from_slice(signature)?;
        secp.verify_schnorr(&sig, msg, xpub)
    }

    /// Get the Verkle proof if present
    pub fn verkle_proof(&self) -> Option<&VerkleProof> {
        self.verkle_proof.as_ref()
    }

    /// Check if this witness contains a Verkle proof
    pub fn has_verkle_proof(&self) -> bool {
        self.verkle_proof.is_some()
    }

    /// Get the inner Bitcoin witness
    pub fn inner(&self) -> &BtcWitness {
        &self.inner
    }

    /// Convert to inner Bitcoin witness
    pub fn into_inner(self) -> BtcWitness {
        self.inner
    }
}

/// Control block for Copperoot script path spending
#[derive(Debug, Clone)]
pub struct CopperootControlBlock {
    /// Control block version (0xC1 for Copperoot)
    pub version: u8,
    /// Parity bit and leaf version
    pub parity_leaf_version: u8,
    /// Internal public key
    pub internal_key: XOnlyPublicKey,
    /// Proof type (1 for Verkle proof)
    pub proof_type: u8,
    /// Verkle proof (if present)
    pub verkle_proof: Option<VerkleProof>,
}

impl CopperootControlBlock {
    /// Create a new control block
    pub fn new(
        parity_leaf_version: u8,
        internal_key: XOnlyPublicKey,
        verkle_proof: Option<VerkleProof>,
    ) -> Self {
        Self {
            version: 0xC1, // Copperoot version
            parity_leaf_version,
            internal_key,
            proof_type: if verkle_proof.is_some() { 1 } else { 0 },
            verkle_proof,
        }
    }

    /// Serialize the control block
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.version);
        data.push(self.parity_leaf_version);
        data.extend_from_slice(&self.internal_key.serialize());
        data.push(self.proof_type);
        
        if let Some(proof) = &self.verkle_proof {
            // Serialize Verkle proof
            data.extend_from_slice(&proof.path.len().to_le_bytes());
            for commitment in &proof.path {
                data.extend_from_slice(&commitment.serialize());
            }
            data.extend_from_slice(&proof.leaf_data.len().to_le_bytes());
            data.extend_from_slice(&proof.leaf_data);
        }
        
        data
    }

    /// Deserialize a control block
    pub fn deserialize(data: &[u8]) -> Result<Self, ControlBlockError> {
        if data.len() < 35 {
            return Err(ControlBlockError::InvalidLength);
        }
        
        let version = data[0];
        if version != 0xC1 {
            return Err(ControlBlockError::InvalidVersion);
        }
        
        let parity_leaf_version = data[1];
        let internal_key = XOnlyPublicKey::from_slice(&data[2..34])?;
        let proof_type = data[34];
        
        let verkle_proof = if proof_type == 1 {
            if data.len() < 35 {
                return Err(ControlBlockError::InvalidLength);
            }
            
            let mut offset = 35;
            
            // Read path length
            if offset + 4 > data.len() {
                return Err(ControlBlockError::InvalidLength);
            }
            let path_len = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;
            
            // Read path commitments
            let mut path = Vec::new();
            for _ in 0..path_len {
                if offset + 33 > data.len() {
                    return Err(ControlBlockError::InvalidLength);
                }
                let commitment = secp256k1::PublicKey::from_slice(&data[offset..offset + 33])?;
                path.push(commitment);
                offset += 33;
            }
            
            // Read leaf data length
            if offset + 4 > data.len() {
                return Err(ControlBlockError::InvalidLength);
            }
            let leaf_data_len = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;
            
            // Read leaf data
            if offset + leaf_data_len > data.len() {
                return Err(ControlBlockError::InvalidLength);
            }
            let leaf_data = data[offset..offset + leaf_data_len].to_vec();
            
            Some(VerkleProof { path, leaf_data })
        } else {
            None
        };
        
        Ok(Self {
            version,
            parity_leaf_version,
            internal_key,
            proof_type,
            verkle_proof,
        })
    }
}

/// Control block errors
#[derive(Debug, thiserror::Error)]
pub enum ControlBlockError {
    #[error("Invalid control block length")]
    InvalidLength,
    #[error("Invalid control block version")]
    InvalidVersion,
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::Keypair;

    #[test]
    fn test_copperoot_witness_key_spend() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let message = Message::from_slice(&[0u8; 32]).unwrap();
        let signature = secp.sign_schnorr(&message, &keypair);
        
        let witness = CopperootWitness::p2tr_key_spend(signature, TapSighashType::Default);
        assert!(!witness.has_verkle_proof());
    }

    #[test]
    fn test_copperoot_witness_script_spend() {
        let script = vec![0x51]; // OP_1
        let control_block = vec![0xC1, 0x00]; // Copperoot version + leaf version
        
        let witness = CopperootWitness::p2tr_script_spend(script, control_block);
        assert!(!witness.has_verkle_proof());
    }

    #[test]
    fn test_copperoot_control_block() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let control_block = CopperootControlBlock::new(0x00, internal_key, None);
        assert_eq!(control_block.version, 0xC1);
        assert_eq!(control_block.proof_type, 0);
        
        let serialized = control_block.serialize();
        let deserialized = CopperootControlBlock::deserialize(&serialized).unwrap();
        assert_eq!(control_block.version, deserialized.version);
        assert_eq!(control_block.proof_type, deserialized.proof_type);
    }
}
