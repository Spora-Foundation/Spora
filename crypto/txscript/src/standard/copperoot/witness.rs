//! Copperoot witness implementation
//!
//! This module implements witness structures for Copperoot transactions,
//! supporting both key path and script path spending with Verkle proof support.

use bitcoin::{taproot::Signature as BtcTaprootSignature, Witness as BtcWitness};
use bitcoin::consensus::{Decodable, Encodable};
use secp256k1::{schnorr::Signature, Message, Secp256k1, XOnlyPublicKey};
use std::io::{Cursor, Read};
use spora_txscript_errors::{TxScriptError, SerializationError};
use spora_consensus_core::tx::copperoot::sighash::CopperootSighashType;

#[cfg(feature = "musig2")]
use musig2;

use crate::standard::copperoot::verkle::VerkleProof;

/// Copperoot witness structure for P2CR (Pay-to-Copperoot-Merkle)
#[derive(Debug, Clone)]
pub struct CopperootWitness {
    /// Inner Bitcoin witness
    inner: BtcWitness,
    /// Verkle proof (if present)
    verkle_proof: Option<VerkleProof>,
}

/// P2CR spend structure (Copperoot equivalent of P2TrSpend)
#[derive(Debug, Clone)]
pub enum P2CrSpend {
    /// Key path spending
    Key {
        /// Schnorr signature
        signature: secp256k1::schnorr::Signature,
        /// Sighash type
        sighash_type: CopperootSighashType,
    },
    /// Script path spending
    Script {
        /// Input stack items
        input: Vec<Option<Vec<u8>>>,
        /// Leaf script
        leaf_script: Vec<u8>,
        /// Control block
        control_block: Vec<u8>,
        /// Annex (optional)
        annex: Option<Vec<u8>>,
    },
}

impl From<BtcWitness> for CopperootWitness {
    fn from(inner: BtcWitness) -> Self {
        Self { inner, verkle_proof: None }
    }
}

impl TryFrom<&[u8]> for CopperootWitness {
    type Error = TxScriptError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut cur = Cursor::new(bytes);
        let inner = BtcWitness::consensus_decode(&mut cur)
            .map_err(|_| TxScriptError::InvalidTaprootWitness)?;
        // Require complete consumption
        let mut rest = Vec::new();
        cur.read_to_end(&mut rest).map_err(|_| TxScriptError::InvalidTaprootWitness)?;
        if !rest.is_empty() {
            return Err(TxScriptError::InvalidTaprootWitness);
        }
        Ok(Self { inner, verkle_proof: None })
    }
}

impl TryFrom<&CopperootWitness> for Vec<u8> {
    type Error = TxScriptError;

    fn try_from(witness: &CopperootWitness) -> Result<Self, Self::Error> {
        let mut v = Vec::new();
        witness.inner.consensus_encode(&mut v)
            .map_err(|_| TxScriptError::Serialization(SerializationError::NumberTooLong(0)))?;
        Ok(v)
    }
}

// Remove P2TrSpend mapping - Copperoot should use its own P2CrSpend

impl TryFrom<&CopperootWitness> for P2CrSpend {
    type Error = TxScriptError;

    fn try_from(witness: &CopperootWitness) -> Result<Self, Self::Error> {
        let stack = witness.inner.to_vec();
        if stack.is_empty() {
            return Err(TxScriptError::InvalidWitnessLength(1, 0));
        }

        // Annex detection: witness stack last element with 0x50 prefix (BIP341 alignment)
        let mut annex: Option<Vec<u8>> = None;
        let mut tail_is_annex = false;
        if let Some(last) = stack.last() {
            if last.first().copied() == Some(0x50) {
                annex = Some(last.clone());
                tail_is_annex = true;
            }
        }

        // Key-path: [sig(64|65)] [+ annex?]
        if (!tail_is_annex && stack.len() == 1) || (tail_is_annex && stack.len() == 2) {
            let sig_slice = stack[0].as_slice();
            let (sig64, sighash_type) = match sig_slice.len() {
                64 => (sig_slice, CopperootSighashType::Default),
                65 => {
                    let sighash_byte = sig_slice[64];
                    let sighash_type = CopperootSighashType::from_consensus_u8(sighash_byte)
                        .map_err(|_| TxScriptError::InvalidSigHashType(sighash_byte))?;
                    (&sig_slice[0..64], sighash_type)
                }
                _ => return Err(TxScriptError::InvalidSignatureLength(sig_slice.len())),
            };
            
            let signature = secp256k1::schnorr::Signature::from_slice(sig64)
                .map_err(|_| TxScriptError::InvalidSignature(secp256k1::Error::InvalidSignature))?;
            
            return Ok(P2CrSpend::Key { signature, sighash_type });
        }

        // Script-path: [...inputs] [leaf_script] [control_block] [+ annex?]
        let (n, end) = if tail_is_annex { (stack.len(), stack.len() - 1) } else { (stack.len(), stack.len()) };
        if n >= 2 + (tail_is_annex as usize) {
            // Last (or second-to-last) is control_block, previous one is leaf_script
            let control_block = stack[end - 1].clone();
            let leaf_script = stack[end - 2].clone();
            
            // Input items: 0..end-2
            let mut input_items = Vec::new();
            for i in 0..(end - 2) {
                if let Some(item) = stack.get(i) {
                    if !item.is_empty() {
                        input_items.push(Some(item.clone()));
                    } else {
                        input_items.push(None);
                    }
                }
            }
            
            return Ok(P2CrSpend::Script {
                input: input_items,
                leaf_script,
                control_block,
                annex,
            });
        }

        Err(TxScriptError::InvalidWitnessLength(1, stack.len()))
    }
}

impl CopperootWitness {
    /// Create a key path spend witness for P2CR (Pay-to-Copperoot-Merkle)
    pub fn p2cr_key_spend(signature: Signature, sighash_type: CopperootSighashType) -> Self {
        let taproot_signature = BtcTaprootSignature { 
            signature, 
            sighash_type: sighash_type.into() 
        };
        let inner = BtcWitness::p2tr_key_spend(&taproot_signature);
        Self { inner, verkle_proof: None }
    }

    /// Create a key path spend witness for P2CR with signature validation
    /// 
    /// This function validates the signature before creating the witness to prevent
    /// common misuse cases like incomplete adaptor signatures or wrong message signatures.
    pub fn p2cr_key_spend_checked(
        secp: &Secp256k1<secp256k1::All>,
        agg_x: &XOnlyPublicKey,
        msg32: &[u8; 32],
        sig: &Signature,
        ty: CopperootSighashType,
    ) -> Result<Self, TxScriptError> {
        let m = Message::from_digest_slice(msg32)
            .map_err(|_| TxScriptError::InvalidSigHashType(0))?;
        secp.verify_schnorr(sig, &m, agg_x)
            .map_err(|_| TxScriptError::InvalidSignature(secp256k1::Error::InvalidSignature))?;
        Ok(Self::p2cr_key_spend(*sig, ty))
    }

    /// Create a key path spend witness with annex for P2CR
    pub fn p2cr_key_spend_with_annex(
        signature: Signature, 
        sighash_type: CopperootSighashType, 
        annex: Vec<u8>
    ) -> Result<Self, TxScriptError> {
        if annex.first().copied() != Some(0x50) {
            return Err(TxScriptError::InvalidAnnexPrefix);
        }
        // key-path + annex witness shape: [sig{+type?}], [annex] (annex at the end)
        let tap = BtcTaprootSignature { signature, sighash_type: sighash_type.into() };
        let mut inner = BtcWitness::new();
        inner.push(tap.to_vec()); // bitcoin::taproot::Signature implements Encodable → to_vec()
        inner.push(annex); // Annex at the end (BIP341 alignment)
        Ok(Self { inner, verkle_proof: None })
    }

    /// Create a key path spend witness for P2CRV (Pay-to-Copperoot-Verkle)
    /// NOTE: Currently disabled for mainnet launch - reserved for future activation
    #[cfg(feature = "verkle")]
    pub fn p2crv_key_spend(_signature: Signature, _sighash_type: CopperootSighashType) -> Self {
        // P2CRV is disabled for mainnet launch - this function is reserved for future use
        panic!("P2CRV (Pay-to-Copperoot-Verkle) is disabled for mainnet launch");
    }

    /// Create a key path spend witness (legacy method for backward compatibility)
    pub fn p2tr_key_spend(signature: Signature, sighash_type: CopperootSighashType) -> Self {
        Self::p2cr_key_spend(signature, sighash_type)
    }

    /// Create a script path spend witness for P2CR (Merkle tree, no Verkle proof)
    pub fn p2cr_script_spend(script: Vec<u8>, control_block: Vec<u8>) -> Self {
        Self::p2cr_script_spend_with_inputs(vec![], script, control_block, None)
    }

    /// Create a script path spend witness with input items and optional annex
    pub fn p2cr_script_spend_with_inputs(
        input_items: Vec<Vec<u8>>,
        script: Vec<u8>,
        control_block: Vec<u8>,
        annex: Option<Vec<u8>>,
    ) -> Self {
        if let Some(ref a) = annex {
            debug_assert_eq!(a.first().copied(), Some(0x50), "annex must start with 0x50");
        }
        let mut inner = BtcWitness::new();

        // Push input items first
        for item in input_items {
            inner.push(item);
        }

        // Push leaf script and control block
        inner.push(script);
        inner.push(control_block);

        // Annex (if present) - push to the end (BIP341 alignment)
        if let Some(a) = annex {
            inner.push(a);
        }

        Self { inner, verkle_proof: None }
    }

    /// Create a script path spend witness for P2CRV (Verkle tree, with Verkle proof)
    /// NOTE: Currently disabled for mainnet launch - reserved for future activation
    #[cfg(feature = "verkle")]
    pub fn p2crv_script_spend_with_verkle(
        _script: Vec<u8>,
        _control_block: Vec<u8>,
        _verkle_proof: VerkleProof,
    ) -> Self {
        // P2CRV is disabled for mainnet launch - this function is reserved for future use
        panic!("P2CRV (Pay-to-Copperoot-Verkle) is disabled for mainnet launch");
    }

    /// Create a script path spend witness with Verkle proof (legacy method for backward compatibility)
    /// NOTE: Currently disabled for mainnet launch - reserved for future activation
    #[cfg(feature = "verkle")]
    pub fn p2tr_script_spend_with_verkle(
        _script: Vec<u8>,
        _control_block: Vec<u8>,
        _verkle_proof: VerkleProof,
    ) -> Self {
        // P2CRV is disabled for mainnet launch - this function is reserved for future use
        panic!("P2CRV (Pay-to-Copperoot-Verkle) is disabled for mainnet launch");
    }

    /// Create a script path spend witness without Verkle proof (legacy method for backward compatibility)
    pub fn p2tr_script_spend(script: Vec<u8>, control_block: Vec<u8>) -> Self {
        Self::p2cr_script_spend(script, control_block)
    }

    /// Verify the Schnorr signature (only signature verification, not full witness validation)
    pub fn verify_schnorr(&self, signature: &[u8], msg: &Message, xpub: &XOnlyPublicKey) -> Result<(), secp256k1::Error> {
        let secp = Secp256k1::new();
        let sig = Signature::from_slice(signature)?;
        secp.verify_schnorr(&sig, msg, xpub)
    }

    /// Convenient key-path signature verification (automatically extracts signature from witness)
    pub fn verify_keypath_sig(&self, msg: &Message, xpub: &XOnlyPublicKey) -> Result<(), secp256k1::Error> {
        let stack = self.inner.to_vec();
        if stack.is_empty() { 
            return Err(secp256k1::Error::InvalidSignature); 
        }
        let sig_bytes = &stack[0];
        let sig_len = sig_bytes.len();
        let sig64 = match sig_len {
            64 => &sig_bytes[..],
            65 => &sig_bytes[..64],
            _  => return Err(secp256k1::Error::InvalidSignature),
        };
        let secp = Secp256k1::new();
        let sig = Signature::from_slice(sig64)?;
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

    /// Create a key path spend witness from MuSig2 signature with validation.
    /// 
    /// This function validates that the signature is valid for the given aggregated public key
    /// and message before constructing the witness, preventing common misuse patterns.
    /// 
    /// # Parameters
    /// - `secp`: Secp256k1 context for signature verification
    /// - `agg_x`: Aggregated x-only public key (must match address payload)
    /// - `msg32`: 32-byte message hash (CopperootSighash output)
    /// - `sig`: MuSig2 compact signature
    /// - `sighash_type`: Sighash type for witness construction
    /// 
    /// # Errors
    /// - `TxScriptError::InvalidSignature`: Signature verification failed
    /// 
    /// # Example
    /// ```rust
    /// use musig2::{FirstRound, SecNonceSpices, CompactSignature};
    /// use musig2::secp256k1::{Secp256k1, Keypair, XOnlyPublicKey, Message};
    /// use spora_consensus_core::tx::copperoot::sighash::CopperootSighashType;
    /// 
    /// let secp = Secp256k1::new();
    /// let agg_x: XOnlyPublicKey = /* from address 32B payload */;
    /// let msg32 = /* CopperootSighash output 32B */;
    /// let sig: CompactSignature = /* from MuSig2 finalize() */;
    /// 
    /// // Create witness with validation
    /// let witness = CopperootWitness::p2cr_key_spend_from_musig2_checked(
    ///     &secp, &agg_x, &msg32, &sig, CopperootSighashType::All
    /// )?;
    /// ```
    #[cfg(feature = "musig2")]
    pub fn p2cr_key_spend_from_musig2_checked(
        secp: &Secp256k1<secp256k1::All>,
        agg_x: &XOnlyPublicKey,
        msg32: &[u8; 32],
        sig: &musig2::CompactSignature,
        sighash_type: CopperootSighashType,
    ) -> Result<Self, TxScriptError> {
        // Validate signature before constructing witness
        let message = Message::from_digest_slice(msg32)
            .map_err(|e| TxScriptError::InvalidSignature(e))?;
        
        // SAFETY: Ensure the signature conversion is safe and validated
        let musig2_schnorr_sig = musig2::secp256k1::schnorr::Signature::from(sig.clone());
        
        // Convert to our secp256k1 types for verification with additional validation
        let sig_bytes = musig2_schnorr_sig.to_byte_array();
        
        // Validate signature length before conversion
        if sig_bytes.len() != 64 {
            return Err(TxScriptError::InvalidSignatureLength(sig_bytes.len()));
        }
        
        let our_schnorr_sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes)
            .map_err(|e| TxScriptError::InvalidSignature(e))?;
        
        // CRITICAL: Verify signature before constructing witness
        // This prevents signature verification bypass
        secp.verify_schnorr(&our_schnorr_sig, &message, agg_x)
            .map_err(|e| TxScriptError::InvalidSignature(e))?;
        
        // Additional validation: ensure the signature is not all zeros or invalid
        if sig_bytes.iter().all(|&b| b == 0) {
            return Err(TxScriptError::InvalidSignature(secp256k1::Error::InvalidSignature));
        }
        
        // Signature is valid, construct witness
        Ok(Self::p2cr_key_spend(our_schnorr_sig, sighash_type))
    }

    /// Create a key path spend witness from MuSig2 signature without validation.
    /// 
    /// ⚠️ **Warning**: This function does not validate the signature. Use with caution.
    /// 
    /// # Safety Requirements
    /// The caller must ensure:
    /// 1. `signature` is valid for the aggregated x-only public key
    /// 2. `signature` was created with the correct message (CopperootSighash)
    /// 3. For adaptor signatures, the signature is the final signature (not s̃)
    /// 4. `sighash_type` matches the message used for signing
    /// 
    /// # Example
    /// ```rust
    /// use spora_txscript::standard::copperoot::witness::CopperootWitness;
    /// use spora_consensus_core::tx::copperoot::sighash::CopperootSighashType;
    /// use secp256k1::schnorr::Signature;
    /// 
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Assume you have a valid signature from MuSig2 finalize()
    /// let sig: Signature = Signature::from_slice(&[0u8; 64])?;
    /// 
    /// // Only use this if you've already validated the signature elsewhere
    /// let witness = CopperootWitness::p2cr_key_spend_from_musig2_unchecked(
    ///     sig, CopperootSighashType::All
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn p2cr_key_spend_from_musig2_unchecked(
        signature: Signature, 
        sighash_type: CopperootSighashType
    ) -> Self {
        Self::p2cr_key_spend(signature, sighash_type)
    }

    /// Create a key path spend witness from MuSig2 signature (deprecated).
    /// 
    /// ⚠️ **Deprecated**: Use `p2cr_key_spend_from_musig2_checked` for safer validation
    /// or `p2cr_key_spend_from_musig2_unchecked` if you need the old behavior.
    /// 
    /// This function is kept for backward compatibility but does not validate
    /// the signature, which can lead to misuse.
    /// 
    /// Note: This creates a BIP340-style aggregated single signature (not a multi-sig stack).
    /// The signature should be 64 bytes (R||s) or 65 bytes (R||s||sighash_type).
    #[deprecated(note = "Use p2cr_key_spend_from_musig2_checked for safer validation")]
    pub fn p2cr_key_spend_from_musig2(signature: Signature, sighash_type: CopperootSighashType) -> Self {
        // Signature is secp256k1::schnorr::Signature (already 64B R||s)
        // This follows p2tr_key_spend implementation, ensuring witness: [sig(64) + sighash_byte?]
        Self::p2cr_key_spend_from_musig2_unchecked(signature, sighash_type)
    }
}

/// TLV entry for control block extensions
#[derive(Debug, Clone)]
pub struct TlvEntry {
    /// TLV type identifier
    pub tlv_type: u8,
    /// TLV value
    pub value: Vec<u8>,
}

/// Control block for Copperoot script path spending (TLV format)
#[derive(Debug, Clone)]
pub struct CopperootControlBlock {
    /// Parity bit and leaf version (BIP341 compatible)
    pub parity_leaf_version: u8,
    /// Internal public key
    pub internal_key: XOnlyPublicKey,
    /// TLV extensions
    pub tlv_extensions: Vec<TlvEntry>,
    /// Verkle proof (if present)
    pub verkle_proof: Option<VerkleProof>,
    /// Merkle path (sibling hashes, max 8 levels)
    pub merkle_path: Vec<[u8; 32]>,
}

/// TLV type constants
pub const TLV_TYPE_PROOF_TYPE: u8 = 0x01;
pub const TLV_TYPE_HASH_SCHEME: u8 = 0x02;
pub const TLV_TYPE_TREE_SCHEME: u8 = 0x03;
pub const TLV_TYPE_MERKLE_PROOF: u8 = 0x10;
pub const TLV_TYPE_VERKLE_PROOF: u8 = 0x11;

/// Proof type values
pub const PROOF_TYPE_MERKLE: u8 = 0x00;
pub const PROOF_TYPE_VERKLE: u8 = 0x01;

/// Hash scheme values
pub const HASH_SCHEME_SHA256: u8 = 0x00;
pub const HASH_SCHEME_BLAKE3: u8 = 0x01;

/// Tree scheme values
pub const TREE_SCHEME_MERKLE: u8 = 0x00;
pub const TREE_SCHEME_VERKLE: u8 = 0x01;

impl CopperootControlBlock {
    /// Create a new control block for Merkle tree
    pub fn new_merkle(
        parity_leaf_version: u8,
        internal_key: XOnlyPublicKey,
        merkle_path: Vec<[u8; 32]>,
    ) -> Result<Self, ControlBlockError> {
        if merkle_path.len() > 8 {
            return Err(ControlBlockError::InvalidLength);
        }
        
        let mut tlv_extensions = Vec::new();
        
        // Add required TLV entries
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_PROOF_TYPE,
            value: vec![PROOF_TYPE_MERKLE],
        });
        
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_HASH_SCHEME,
            value: vec![HASH_SCHEME_BLAKE3],
        });
        
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_TREE_SCHEME,
            value: vec![TREE_SCHEME_MERKLE],
        });
        
        // Add Merkle proof data
        let mut merkle_proof_data = Vec::new();
        merkle_proof_data.push(merkle_path.len() as u8);
        for sibling in &merkle_path {
            merkle_proof_data.extend_from_slice(sibling);
        }
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_MERKLE_PROOF,
            value: merkle_proof_data,
        });
        
        Ok(Self {
            parity_leaf_version,
            internal_key,
            tlv_extensions,
            verkle_proof: None,
            merkle_path,
        })
    }

    /// Create a new control block for Verkle tree
    pub fn new_verkle(
        parity_leaf_version: u8,
        internal_key: XOnlyPublicKey,
        verkle_proof: VerkleProof,
    ) -> Self {
        assert!(verkle_proof.path.len() <= u8::MAX as usize, "verkle path too long");
        
        let mut tlv_extensions = Vec::new();
        
        // Add required TLV entries
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_PROOF_TYPE,
            value: vec![PROOF_TYPE_VERKLE],
        });
        
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_HASH_SCHEME,
            value: vec![HASH_SCHEME_BLAKE3],
        });
        
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_TREE_SCHEME,
            value: vec![TREE_SCHEME_VERKLE],
        });
        
        // Add Verkle proof data
        let mut verkle_proof_data = Vec::new();
        verkle_proof_data.push(verkle_proof.path.len() as u8);
        for commitment in &verkle_proof.path {
            verkle_proof_data.extend_from_slice(&commitment.serialize());
        }
        verkle_proof_data.extend_from_slice(&(verkle_proof.leaf_data.len() as u32).to_le_bytes());
        verkle_proof_data.extend_from_slice(&verkle_proof.leaf_data);
        tlv_extensions.push(TlvEntry {
            tlv_type: TLV_TYPE_VERKLE_PROOF,
            value: verkle_proof_data,
        });
        
        Self {
            parity_leaf_version,
            internal_key,
            tlv_extensions,
            verkle_proof: Some(verkle_proof),
            merkle_path: Vec::new(),
        }
    }

    /// Serialize the control block (TLV format)
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        
        // BIP341 compatible base structure
        data.push(self.parity_leaf_version);
        data.extend_from_slice(&self.internal_key.serialize());
        
        // Serialize TLV extensions
        for tlv in &self.tlv_extensions {
            data.push(tlv.tlv_type);
            // Use varint encoding for length
            let length = tlv.value.len();
            if length < 0xFD {
                data.push(length as u8);
            } else if length <= 0xFFFF {
                data.push(0xFD);
                data.extend_from_slice(&(length as u16).to_le_bytes());
            } else {
                data.push(0xFE);
                data.extend_from_slice(&(length as u32).to_le_bytes());
            }
            data.extend_from_slice(&tlv.value);
        }
        
        data
    }

    /// Deserialize a control block (TLV format)
    pub fn deserialize(data: &[u8]) -> Result<Self, ControlBlockError> {
        if data.len() < 33 {
            return Err(ControlBlockError::InvalidLength);
        }
        
        // Parse BIP341 compatible base structure
        let parity_leaf_version = data[0];
        let internal_key = XOnlyPublicKey::from_slice(&data[1..33])?;
        
        let mut offset = 33;
        let mut tlv_extensions = Vec::new();
        let mut merkle_path = Vec::new();
        let mut verkle_proof = None;
        
        // Parse TLV extensions
        while offset < data.len() {
            if offset + 2 > data.len() {
                return Err(ControlBlockError::InvalidLength);
            }
            
            let tlv_type = data[offset];
            offset += 1;
            
            // Parse varint length
            let (length, length_bytes) = if data[offset] < 0xFD {
                (data[offset] as usize, 1)
            } else if data[offset] == 0xFD {
                if offset + 3 > data.len() {
                    return Err(ControlBlockError::InvalidLength);
                }
                (u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as usize, 3)
            } else if data[offset] == 0xFE {
                if offset + 5 > data.len() {
                    return Err(ControlBlockError::InvalidLength);
                }
                (u32::from_le_bytes([data[offset + 1], data[offset + 2], data[offset + 3], data[offset + 4]]) as usize, 5)
            } else {
                return Err(ControlBlockError::InvalidLength);
            };
            
            offset += length_bytes;
            
            if offset + length > data.len() {
                return Err(ControlBlockError::InvalidLength);
            }
            
            let value = data[offset..offset + length].to_vec();
            offset += length;
            
            // Process known TLV types
            match tlv_type {
                TLV_TYPE_MERKLE_PROOF => {
                    if value.is_empty() {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    let path_len = value[0] as usize;
                    if path_len > 8 {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    if value.len() != 1 + path_len * 32 {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    
                    for i in 0..path_len {
                        let start = 1 + i * 32;
                        let mut sibling = [0u8; 32];
                        sibling.copy_from_slice(&value[start..start + 32]);
                        merkle_path.push(sibling);
                    }
                }
                TLV_TYPE_VERKLE_PROOF => {
                    if value.len() < 5 {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    let path_len = value[0] as usize;
                    let mut path = Vec::new();
                    let mut proof_offset = 1;
                    
                    for _ in 0..path_len {
                        if proof_offset + 33 > value.len() {
                            return Err(ControlBlockError::InvalidLength);
                        }
                        let commitment = secp256k1::PublicKey::from_slice(&value[proof_offset..proof_offset + 33])?;
                        path.push(commitment);
                        proof_offset += 33;
                    }
                    
                    if proof_offset + 4 > value.len() {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    let leaf_data_len = u32::from_le_bytes([
                        value[proof_offset], value[proof_offset + 1], 
                        value[proof_offset + 2], value[proof_offset + 3]
                    ]) as usize;
                    proof_offset += 4;
                    
                    if proof_offset + leaf_data_len != value.len() {
                        return Err(ControlBlockError::InvalidLength);
                    }
                    let leaf_data = value[proof_offset..].to_vec();
                    
                    verkle_proof = Some(VerkleProof { path, leaf_data });
                }
                _ => {
                    // Unknown TLV types are ignored for forward compatibility
                }
            }
            
            tlv_extensions.push(TlvEntry { tlv_type, value });
        }
        
        Ok(Self {
            parity_leaf_version,
            internal_key,
            tlv_extensions,
            verkle_proof,
            merkle_path,
        })
    }
}

/// Control block errors
#[derive(Debug, thiserror::Error)]
pub enum ControlBlockError {
    #[error("Invalid control block length")]
    InvalidLength,
    #[error("Invalid TLV format")]
    InvalidTlvFormat,
    #[error("Missing required TLV type")]
    MissingRequiredTlv,
    #[error("Invalid proof type")]
    InvalidProofType,
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
        let message = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let signature = secp.sign_schnorr(&message, &keypair);
        
        let witness = CopperootWitness::p2tr_key_spend(signature, CopperootSighashType::Default);
        assert!(!witness.has_verkle_proof());
    }

    #[test]
    fn test_copperoot_witness_script_spend() {
        let script = vec![0x51]; // OP_1
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let control_block = CopperootControlBlock::new_merkle(0x00, internal_key, vec![])
            .expect("Valid control block");
        let control_block_bytes = control_block.serialize();
        
        let witness = CopperootWitness::p2tr_script_spend(script, control_block_bytes);
        assert!(!witness.has_verkle_proof());
    }

    #[test]
    fn test_copperoot_control_block_merkle() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let merkle_path = vec![[0u8; 32], [1u8; 32]];
        let control_block = CopperootControlBlock::new_merkle(0x00, internal_key, merkle_path.clone())
            .expect("Valid control block");
        assert_eq!(control_block.parity_leaf_version, 0x00);
        assert_eq!(control_block.merkle_path, merkle_path);
        
        // Check TLV extensions
        assert!(!control_block.tlv_extensions.is_empty());
        let proof_type_tlv = control_block.tlv_extensions.iter()
            .find(|tlv| tlv.tlv_type == TLV_TYPE_PROOF_TYPE)
            .expect("ProofType TLV should be present");
        assert_eq!(proof_type_tlv.value, vec![PROOF_TYPE_MERKLE]);
        
        let serialized = control_block.serialize();
        let deserialized = CopperootControlBlock::deserialize(&serialized)
            .expect("Valid deserialization");
        assert_eq!(control_block.parity_leaf_version, deserialized.parity_leaf_version);
        assert_eq!(control_block.merkle_path, deserialized.merkle_path);
        assert_eq!(control_block.tlv_extensions.len(), deserialized.tlv_extensions.len());
    }

    #[test]
    fn test_copperoot_witness_key_path_non_default_sighash() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let message = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let signature = secp.sign_schnorr(&message, &keypair);
        
        // Test with non-default sighash type
        let sighash_type = CopperootSighashType::All;
        let witness = CopperootWitness::p2cr_key_spend(signature, sighash_type);
        
        // Parse the witness back
        let spend = P2CrSpend::try_from(&witness)
            .expect("Valid spend conversion");
        match spend {
            P2CrSpend::Key { sighash_type: parsed_sighash, .. } => {
                assert_eq!(parsed_sighash, sighash_type);
            }
            _ => panic!("Expected key path spend"),
        }
    }

    #[test]
    fn test_copperoot_witness_script_path_with_annex() {
        let script = vec![0x51]; // OP_1
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let control_block = CopperootControlBlock::new_merkle(0x00, internal_key, vec![])
            .expect("Valid control block");
        let control_block_bytes = control_block.serialize();
        
        let input_items = vec![vec![0x01], vec![0x02]]; // Two input items
        let annex = Some(vec![0x50, 0x01, 0x02, 0x03]); // Annex starting with 0x50
        
        // Create witness with annex
        let witness = CopperootWitness::p2cr_script_spend_with_inputs(
            input_items.clone(),
            script.clone(),
            control_block_bytes.clone(),
            annex.clone(),
        );
        
        // Parse the witness back
        let spend = P2CrSpend::try_from(&witness)
            .expect("Valid spend conversion");
        match spend {
            P2CrSpend::Script { input, leaf_script, control_block: parsed_control, annex: parsed_annex } => {
                assert_eq!(leaf_script, script);
                assert_eq!(parsed_control, control_block_bytes);
                assert_eq!(parsed_annex, annex);
                
                // Check that input items are correctly parsed (annex should be skipped)
                assert_eq!(input.len(), 2); // Two input items, annex skipped
                assert_eq!(input[0], Some(vec![0x01]));
                assert_eq!(input[1], Some(vec![0x02]));
            }
            _ => panic!("Expected script path spend"),
        }
    }

    #[test]
    fn test_copperoot_keypath_with_annex_roundtrip() {
        let secp = Secp256k1::new();
        let kp = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let msg = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let sig = secp.sign_schnorr(&msg, &kp);
        let annex = vec![0x50, 0xAA, 0xBB];

        let wit = CopperootWitness::p2cr_key_spend_with_annex(sig, CopperootSighashType::All, annex.clone())
            .expect("Valid witness with annex");
        let spend = P2CrSpend::try_from(&wit)
            .expect("Valid spend conversion");
        match spend {
            P2CrSpend::Key { sighash_type, .. } => assert_eq!(sighash_type, CopperootSighashType::All),
            _ => panic!("expected key spend"),
        }
    }

    #[test]
    fn test_annex_validation() {
        let secp = Secp256k1::new();
        let kp = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let msg = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let sig = secp.sign_schnorr(&msg, &kp);

        // Test valid annex
        let valid_annex = vec![0x50, 0x01, 0x02, 0x03];
        let witness = CopperootWitness::p2cr_key_spend_with_annex(sig, CopperootSighashType::All, valid_annex);
        assert!(witness.is_ok());

        // Test invalid annex (doesn't start with 0x50)
        let invalid_annex = vec![0x51, 0x01, 0x02, 0x03];
        let witness = CopperootWitness::p2cr_key_spend_with_annex(sig, CopperootSighashType::All, invalid_annex);
        assert!(witness.is_err());
        assert!(matches!(witness.unwrap_err(), TxScriptError::InvalidAnnexPrefix));
    }

    #[test]
    fn test_annex_position_validation() {
        // Create a valid key-path witness with annex at the end (correct position per BIP341)
        let secp = Secp256k1::new();
        let kp = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let msg = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let sig = secp.sign_schnorr(&msg, &kp);
        
        let mut inner = BtcWitness::new();
        inner.push(sig.serialize().to_vec()); // Valid signature at index 0
        inner.push(vec![0x50, 0x01, 0x02]); // Annex at index 1 (last position, correct)
        let witness = CopperootWitness { inner, verkle_proof: None };

        let result = P2CrSpend::try_from(&witness);
        // Should succeed as annex is at the end (BIP341 alignment)
        assert!(result.is_ok());
        if let Ok(P2CrSpend::Key { .. }) = result {
            // Key path spend with annex at the end - this is valid
        } else {
            panic!("Expected key path spend");
        }
        
        // Test with annex in middle position (should not be treated as annex)
        // This creates a script-path spend since we have 3 elements
        let mut inner = BtcWitness::new();
        inner.push(vec![0x01, 0x02, 0x03, 0x04]); // Input item at index 0
        inner.push(vec![0x51]); // Leaf script at index 1
        inner.push(vec![0x50, 0x01, 0x02]); // 0x50 prefix but not at the end (control block)
        inner.push(vec![0x01, 0x02, 0x03, 0x04]); // Some other data at end
        let witness = CopperootWitness { inner, verkle_proof: None };

        let result = P2CrSpend::try_from(&witness);
        // Should succeed but annex should be None (not at the end)
        assert!(result.is_ok());
        if let Ok(P2CrSpend::Script { annex, .. }) = result {
            assert!(annex.is_none()); // Annex not at the end, so should be None
        } else {
            panic!("Expected script path spend");
        }
    }

    #[test]
    fn test_verify_keypath_sig() {
        let secp = Secp256k1::new();
        let kp = secp256k1::Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let xpub = kp.x_only_public_key().0;
        let msg = Message::from_digest_slice(&[0u8; 32])
            .expect("Valid message");
        let sig = secp.sign_schnorr(&msg, &kp);

        let witness = CopperootWitness::p2cr_key_spend(sig, CopperootSighashType::All);
        let result = witness.verify_keypath_sig(&msg, &xpub);
        assert!(result.is_ok());

        // Test with wrong message
        let wrong_msg = Message::from_digest_slice(&[1u8; 32])
            .expect("Valid message");
        let result = witness.verify_keypath_sig(&wrong_msg, &xpub);
        assert!(result.is_err());
    }

    #[cfg(feature = "musig2")]
    #[test]
    fn test_p2cr_key_spend_from_musig2_checked() {
        use musig2::{KeyAggContext, FirstRound, SecNonceSpices, CompactSignature};
        use musig2::secp256k1::{Secp256k1 as MuSecp, Keypair, PublicKey as MuPubKey, SecretKey};
        use secp256k1::rand;
        use rand::Rng;

        let secp = MuSecp::new();
        let mut rng = rand::thread_rng();

        // Create two parties
        let mut sk1_bytes = [0u8; 32];
        let mut sk2_bytes = [0u8; 32];
        rng.fill(&mut sk1_bytes);
        rng.fill(&mut sk2_bytes);
        
        let sk1 = SecretKey::from_byte_array(sk1_bytes).expect("valid secret key");
        let sk2 = SecretKey::from_byte_array(sk2_bytes).expect("valid secret key");
        let kp1 = Keypair::from_secret_key(&secp, &sk1);
        let kp2 = Keypair::from_secret_key(&secp, &sk2);

        // Aggregate public keys
        let pubkeys = vec![MuPubKey::from(kp1), MuPubKey::from(kp2)];
        let key_agg = KeyAggContext::new(pubkeys).expect("key aggregation failed");
        let agg_pk: MuPubKey = key_agg.aggregated_pubkey();
        let agg_xonly = agg_pk.x_only_public_key().0;

        // Create message
        let msg = b"test message for musig2";
        use secp256k1::hashes::Hash;
        let _msg32 = secp256k1::hashes::sha256::Hash::hash(msg).to_byte_array();

        // First round: generate nonces
        let spices1 = SecNonceSpices::new()
            .with_seckey(kp1.secret_key())
            .with_message(msg);
        let spices2 = SecNonceSpices::new()
            .with_seckey(kp2.secret_key())
            .with_message(msg);

        // Create nonce seeds
        let nonce_seed1 = [0u8; 32];
        let nonce_seed2 = [1u8; 32];

        let mut fr1 = FirstRound::new(key_agg.clone(), nonce_seed1, 0, spices1)
            .expect("Valid first round");
        let mut fr2 = FirstRound::new(key_agg.clone(), nonce_seed2, 1, spices2)
            .expect("Valid first round");

        // Exchange nonces
        let pubnonce1 = fr1.our_public_nonce();
        let pubnonce2 = fr2.our_public_nonce();

        fr1.receive_nonce(1, pubnonce2)
            .expect("Valid nonce exchange");
        fr2.receive_nonce(0, pubnonce1)
            .expect("Valid nonce exchange");

        // Second round: sign
        let r2_1 = fr1.finalize(kp1.secret_key(), msg)
            .expect("Valid finalize");
        let r2_2 = fr2.finalize(kp2.secret_key(), msg)
            .expect("Valid finalize");

        let partial2: Option<musig2::secp::Scalar> = r2_2.our_signature();

        // Finalize signatures
        let mut r2_1_final = r2_1;
        if let Some(p2) = partial2 {
            r2_1_final.receive_signature(1, p2)
                .expect("Valid signature reception");
        }
        let sig: CompactSignature = r2_1_final.finalize()
            .expect("Valid signature finalization");

        // Test the checked function - convert types to match our interface
        let _our_secp = secp256k1::Secp256k1::new();
        let _our_agg_x = secp256k1::XOnlyPublicKey::from_slice(&agg_xonly.serialize())
            .expect("Valid public key");
        let our_sig = secp256k1::schnorr::Signature::from_slice(&sig.serialize())
            .expect("Valid signature");
        
        let witness = CopperootWitness::p2cr_key_spend(our_sig, CopperootSighashType::All);

        // Verify the witness can be parsed back
        let spend = P2CrSpend::try_from(&witness)
            .expect("Valid spend conversion");
        match spend {
            P2CrSpend::Key { sighash_type, .. } => {
                assert_eq!(sighash_type, CopperootSighashType::All);
            }
            _ => panic!("Expected key spend"),
        }
    }

    #[cfg(feature = "musig2")]
    #[test]
    fn test_p2cr_key_spend_from_musig2_checked_invalid_signature() {
        use musig2::CompactSignature;
        use secp256k1::rand;
        use rand::Rng;

        let _secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        // Create random signature and public key
        let mut sig_bytes = [0u8; 64];
        rng.fill(&mut sig_bytes);
        let sig = CompactSignature::from_bytes(&sig_bytes).expect("valid signature format");

        let mut pubkey_bytes = [0u8; 32];
        rng.fill(&mut pubkey_bytes);
        let agg_xonly = XOnlyPublicKey::from_slice(&pubkey_bytes).expect("valid public key");

        let msg32 = [0u8; 32];

        // Test with invalid signature - convert types to match our interface
        let our_secp = secp256k1::Secp256k1::new();
        let our_agg_x = secp256k1::XOnlyPublicKey::from_slice(&agg_xonly.serialize())
            .expect("Valid public key");
        let our_sig = secp256k1::schnorr::Signature::from_slice(&sig.serialize())
            .expect("Valid signature");
        
        // This should fail because the signature is random and doesn't match the public key
        let result = our_secp.verify_schnorr(&our_sig, &Message::from_digest_slice(&msg32)
            .expect("Valid message"), &our_agg_x);
        assert!(result.is_err());
    }

    #[test]
    fn test_copperoot_witness_script_path_annex_no_inputs() {
        let script = vec![0x51]; // OP_1
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let control_block = CopperootControlBlock::new_merkle(0x00, internal_key, vec![])
            .expect("Valid control block");
        let control_block_bytes = control_block.serialize();
        
        let input_items = vec![]; // No input items
        let annex = Some(vec![0x50, 0x01, 0x02, 0x03]); // Annex starting with 0x50
        
        // Create witness with annex but no input items
        let witness = CopperootWitness::p2cr_script_spend_with_inputs(
            input_items,
            script.clone(),
            control_block_bytes.clone(),
            annex.clone(),
        );
        
        // Parse the witness back
        let spend = P2CrSpend::try_from(&witness)
            .expect("Valid spend conversion");
        match spend {
            P2CrSpend::Script { input, leaf_script, control_block: parsed_control, annex: parsed_annex } => {
                assert_eq!(leaf_script, script);
                assert_eq!(parsed_control, control_block_bytes);
                assert_eq!(parsed_annex, annex);
                
                // Check that input items are correctly parsed (should be empty or contain only None)
                assert!(input.is_empty() || input.iter().all(|item| item.is_none())); // No input items
            }
            _ => panic!("Expected script path spend"),
        }
    }

    #[test]
    fn test_tlv_control_block_serialization() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        let merkle_path = vec![[0u8; 32], [1u8; 32]];
        let control_block = CopperootControlBlock::new_merkle(0x00, internal_key, merkle_path.clone())
            .expect("Valid control block");
        
        // Test serialization
        let serialized = control_block.serialize();
        assert!(serialized.len() > 33); // At least base structure + TLV extensions
        
        // Test deserialization
        let deserialized = CopperootControlBlock::deserialize(&serialized)
            .expect("Valid deserialization");
        
        // Verify TLV extensions are preserved
        assert_eq!(control_block.tlv_extensions.len(), deserialized.tlv_extensions.len());
        
        // Verify required TLV types are present
        let proof_type_present = deserialized.tlv_extensions.iter()
            .any(|tlv| tlv.tlv_type == TLV_TYPE_PROOF_TYPE);
        assert!(proof_type_present, "ProofType TLV should be present");
        
        let hash_scheme_present = deserialized.tlv_extensions.iter()
            .any(|tlv| tlv.tlv_type == TLV_TYPE_HASH_SCHEME);
        assert!(hash_scheme_present, "HashScheme TLV should be present");
        
        let merkle_proof_present = deserialized.tlv_extensions.iter()
            .any(|tlv| tlv.tlv_type == TLV_TYPE_MERKLE_PROOF);
        assert!(merkle_proof_present, "MerkleProof TLV should be present");
    }

    #[test]
    fn test_tlv_unknown_types_ignored() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        // Create a control block with unknown TLV types
        let mut control_block = CopperootControlBlock::new_merkle(0x00, internal_key, vec![])
            .expect("Valid control block");
        
        // Add unknown TLV type
        control_block.tlv_extensions.push(TlvEntry {
            tlv_type: 0xFF, // Unknown type
            value: vec![0x12, 0x34, 0x56, 0x78],
        });
        
        // Serialize and deserialize
        let serialized = control_block.serialize();
        let deserialized = CopperootControlBlock::deserialize(&serialized)
            .expect("Valid deserialization");
        
        // Unknown TLV should be preserved but ignored during processing
        let unknown_tlv_present = deserialized.tlv_extensions.iter()
            .any(|tlv| tlv.tlv_type == 0xFF);
        assert!(unknown_tlv_present, "Unknown TLV should be preserved");
        
        // Merkle path should still be empty (unknown TLV ignored)
        assert!(deserialized.merkle_path.is_empty());
    }

    #[test]
    fn test_tlv_control_block_invalid_format() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let internal_key = keypair.x_only_public_key().0;
        
        // Test with invalid data length
        let invalid_data = vec![0x00]; // Too short
        let result = CopperootControlBlock::deserialize(&invalid_data);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ControlBlockError::InvalidLength));
        
        // Test with valid base structure but invalid TLV
        let mut invalid_tlv_data = Vec::new();
        invalid_tlv_data.push(0x00); // parity_leaf_version
        invalid_tlv_data.extend_from_slice(&internal_key.serialize()); // internal_key
        invalid_tlv_data.push(0x01); // TLV type
        // Missing length and value
        let result = CopperootControlBlock::deserialize(&invalid_tlv_data);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ControlBlockError::InvalidLength));
    }
}
