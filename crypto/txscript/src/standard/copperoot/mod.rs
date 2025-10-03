//! Copperoot implementation - Taproot variant with BLAKE3, Verkle trees, and native MuSig2
//!
//! This module implements Copperoot, a modified version of Taproot that:
//! - Uses BLAKE3-256 for all hashing operations
//! - Supports Verkle trees with 8-layer depth limit
//! - Includes native MuSig2 support
//! - Maintains compatibility with existing Taproot functionality

pub mod hasher;
pub mod verkle;
#[cfg(feature = "musig2")]
pub mod musig2;
pub mod witness;
pub mod sighash;

// Re-export MuSig2 types for easier access
#[cfg(feature = "musig2")]
pub use musig2::{MuSig2KeyAgg, MuSig2Nonce, MuSig2Session, MuSig2Signature, EncryptedSignature, MuSig2Error};

use crate::TxScriptError;
use secp256k1::XOnlyPublicKey;
use tondi_consensus_core::tx::VerifiableTransaction;
use blake3::Hasher;
use crate::standard::copperoot::sighash::CopperootLeafHash;
use crate::standard::copperoot::witness::{CopperootWitness, P2CrSpend, CopperootControlBlock};

/// Abstract trait for Taproot-like execution semantics
/// This allows us to reuse the execution flow while parameterizing
/// the specific hash functions, control block formats, and verification logic
pub trait ScriptVariant {
    /// Witness structure for this Taproot-like variant
    type Witness;
    
    /// Parse witness from signature script
    fn parse_witness(sig_script: &[u8]) -> Result<Self::Witness, TxScriptError>;
    
    /// Verify commitment for script path spending
    fn verify_commitment(
        xpub: XOnlyPublicKey, 
        leaf_script: &[u8], 
        control_block: &[u8]
    ) -> Result<(), TxScriptError>;
    
    /// Compute key spend signature hash
    fn key_spend_sighash<T: VerifiableTransaction>(
        tx: &T, 
        idx: usize
    ) -> Result<secp256k1::Message, TxScriptError>;
    
    /// Extract signature from key spend witness
    fn extract_key_spend_signature(witness: &Self::Witness) -> Result<Vec<u8>, TxScriptError>;
    
    /// Extract script path components from witness
    fn extract_script_spend_components(
        witness: &Self::Witness
    ) -> Result<(Vec<Vec<u8>>, Vec<u8>, Vec<u8>), TxScriptError>;
}

/// Copperoot implementation of ScriptVariant trait
pub struct CopperootVariant;

impl ScriptVariant for CopperootVariant {
    type Witness = CopperootWitness;

    fn parse_witness(sig_script: &[u8]) -> Result<Self::Witness, TxScriptError> {
        CopperootWitness::try_from(sig_script)
    }

    fn verify_commitment(
        xpub: XOnlyPublicKey,
        leaf_script: &[u8],
        control_block: &[u8],
    ) -> Result<(), TxScriptError> {
        let control_block = CopperootControlBlock::deserialize(control_block)
            .map_err(|_| TxScriptError::InvalidTaprootWitness)?;

        // Verify TLV extensions contain required proof type
        let proof_type = control_block.tlv_extensions.iter()
            .find(|tlv| tlv.tlv_type == crate::standard::copperoot::witness::TLV_TYPE_PROOF_TYPE)
            .and_then(|tlv| tlv.value.first().copied())
            .ok_or(TxScriptError::InvalidTaprootWitness)?;

        // Verify proof type (0 for Merkle, 1 for Verkle)
        if proof_type > 1 {
            return Err(TxScriptError::InvalidTaprootWitness);
        }

        if proof_type == 0 {
            // Merkle proof verification
            verify_merkle_commitment(xpub, leaf_script, &control_block)
        } else {
            // Verkle proof verification (currently disabled)
            Err(TxScriptError::OpcodeDisabled("P2CRV (Verkle) is disabled for mainnet launch".to_string()))
        }
    }

    fn key_spend_sighash<T: VerifiableTransaction>(
        tx: &T,
        idx: usize,
    ) -> Result<secp256k1::Message, TxScriptError> {
        use crate::standard::copperoot::sighash::{SighashCache, Prevouts, CopperootSighashType};
        
        let mut sighasher = SighashCache::new(tx.tx());
        let vouts = tx
            .populated_inputs()
            .map(|(_, utxo)| tondi_consensus_core::tx::TransactionOutput {
                value: utxo.amount,
                script_public_key: utxo.script_public_key.clone(),
            })
            .collect::<Vec<_>>();
        let prevouts = Prevouts::All(&vouts);
        let sighash = match sighasher
            .copperoot_key_spend_signature_hash(idx, &prevouts, CopperootSighashType::Default) {
            Ok(sighash) => sighash,
            Err(_) => return Err(TxScriptError::InvalidSignature(secp256k1::Error::InvalidSignature)),
        };
        Ok(secp256k1::Message::from(sighash))
    }

    fn extract_key_spend_signature(witness: &Self::Witness) -> Result<Vec<u8>, TxScriptError> {
        let p2cr = P2CrSpend::try_from(witness)?;
        match p2cr {
            P2CrSpend::Key { signature, .. } => Ok(signature.as_ref().to_vec()),
            _ => Err(TxScriptError::InvalidTaprootWitness),
        }
    }

    fn extract_script_spend_components(
        witness: &Self::Witness,
    ) -> Result<(Vec<Vec<u8>>, Vec<u8>, Vec<u8>), TxScriptError> {
        let p2cr = P2CrSpend::try_from(witness)?;
        match p2cr {
            P2CrSpend::Script { input, leaf_script, control_block, .. } => {
                let input_items = input.into_iter()
                    .map(|item| item.unwrap_or_default())
                    .collect();
                Ok((input_items, leaf_script, control_block))
            }
            _ => Err(TxScriptError::InvalidTaprootWitness),
        }
    }
}

/// Verify Merkle commitment using BLAKE3-256 with complete tweak validation
fn verify_merkle_commitment(
    xpub: XOnlyPublicKey,
    leaf_script: &[u8],
    control_block: &CopperootControlBlock,
) -> Result<(), TxScriptError> {
    // 1) Basic constraints
    if control_block.merkle_path.len() > 8 {
        return Err(TxScriptError::InvalidTaprootWitness);
    }
    
    // Verify TLV extensions contain required proof type
    let proof_type = control_block.tlv_extensions.iter()
        .find(|tlv| tlv.tlv_type == crate::standard::copperoot::witness::TLV_TYPE_PROOF_TYPE)
        .and_then(|tlv| tlv.value.first().copied())
        .ok_or(TxScriptError::InvalidTaprootWitness)?;
    
    if proof_type > 1 {
        return Err(TxScriptError::InvalidTaprootWitness);
    }

    // 2) Parse parity / leaf_version (bit7 = parity, low 7 bits = leaf_version)
    let parity_bit = (control_block.parity_leaf_version & 0x80) != 0;
    let leaf_version = control_block.parity_leaf_version & 0x7F;

    // 3) Compute leaf hash (BLAKE3-256, consistent with existing implementation)
    let leaf_hash = compute_copperoot_leaf_hash(leaf_script, leaf_version);

    // 4) Compute merkle root bottom-up (BLAKE3-256)
    let mut root = leaf_hash;
    for sibling in &control_block.merkle_path {
        root = compute_copperoot_node_hash(&root, sibling);
    }

    // 5) Compute Copperoot Tweak = BLAKE3("CopperTweak" || P || root || [proof_type])
    //    Note: tweak must be passed as scalar to add_tweak; if >= n will return Err → reject directly
    let mut tweak_hasher = Hasher::new();
    tweak_hasher.update(b"CopperTweak");
    tweak_hasher.update(&control_block.internal_key.serialize());
    tweak_hasher.update(&root);
    tweak_hasher.update(&[proof_type]);
    let tweak_bytes = *tweak_hasher.finalize().as_bytes(); // [u8; 32]

    // 6) Compute Q = P + tweak*G, and take xonly(Q)
    let secp = secp256k1::Secp256k1::verification_only();

    // Pass tweak as scalar; if invalid (>= n / 0) will Err
    let tweak_scalar = secp256k1::Scalar::from_be_bytes(tweak_bytes)
        .map_err(|_| TxScriptError::InvalidTaprootWitness)?;

    // add_tweak returns (XOnlyPublicKey, bool_parity). Here parity_true represents the boolean flag
    // from libsecp calculation indicating "whether to take the opposite point to get x-only canonical representation" (corresponding to BIP340 semantics).
    let (tweaked_xonly, actual_parity) =
        control_block.internal_key.add_tweak(&secp, &tweak_scalar)
        .map_err(|_| TxScriptError::InvalidTaprootWitness)?;

    // 7) Compare x-only output key with xpub in script public key
    if tweaked_xonly != xpub {
        return Err(TxScriptError::InvalidTaprootWitness);
    }

    // 8) Verify parity bit: parity bit in control block must match actual calculation
    //
    // Note: For rust-secp256k1:
    //  - XOnlyPublicKey::from_pubkey(..) returns (xonly, parity).
    //  - add_tweak(..) returns (xonly, Parity).
    // Here, we use the parity returned by from_pubkey as the "output x-only parity" to align with the control block bit.
    let parity_as_bool = matches!(actual_parity, secp256k1::Parity::Odd);
    if parity_as_bool != parity_bit {
        return Err(TxScriptError::InvalidTaprootWitness);
    }

    Ok(())
}

/// Compute Copperoot leaf hash using BLAKE3-256
fn compute_copperoot_leaf_hash(script: &[u8], leaf_version: u8) -> [u8; 32] {
    let mut hasher = CopperootLeafHash::engine();
    hasher.update(&[leaf_version]);
    hasher.update(&(script.len() as u32).to_le_bytes());
    hasher.update(script);
    CopperootLeafHash::from_engine(hasher).to_byte_array()
}

/// Compute Copperoot node hash using BLAKE3-256
fn compute_copperoot_node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootNode");
    
    // Sort the hashes for deterministic ordering
    if left < right {
        hasher.update(left);
        hasher.update(right);
    } else {
        hasher.update(right);
        hasher.update(left);
    }
    
    hasher.finalize().into()
}

// Remove unused imports

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Keypair, Message, Secp256k1};
    use bitcoin::Witness as BtcWitness;
    use smallvec::SmallVec;
    use std::str::FromStr;
    use tondi_consensus_core::{
        subnets::SubnetworkId,
        tx::{ScriptPublicKey, TransactionId, TransactionOutpoint, TransactionInput, TransactionOutput, Transaction, UtxoEntry, PopulatedTransaction},
        hashing::sighash::SigHashReusedValuesUnsync,
    };
    use hex;
    use crate::{Cache, TxScriptEngine, MAX_SCRIPT_PUBLIC_KEY_VERSION, SCRIPT_VER_P2CR};
    use crate::standard::copperoot::sighash::{Prevouts, SighashCache, CopperootSighashType};
    use crate::standard::{OpTrue, OpData32};

    #[test]
    fn test_copperoot_key_spend() {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            &hex::decode("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160")
                .expect("Valid hex string"),
        )
        .expect("Valid private key");
        // Use Copperoot P2CR script generation
        let xonly_pubkey = keypair.x_only_public_key().0;
        let script_pub_key = SmallVec::from_iter([OpTrue, OpData32].into_iter().chain(xonly_pubkey.serialize()));

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3")
            .expect("Valid transaction ID");

        let mut tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, script_pub_key.clone()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let utxos = vec![TransactionOutput::new(100, ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, script_pub_key.clone()))];
        let prevouts = Prevouts::All(&utxos);
        let input_index = 0;
        let sighash_type = CopperootSighashType::Default;
        let mut sighasher = SighashCache::new(&tx);
        let sighash =
            sighasher.copperoot_key_spend_signature_hash(input_index, &prevouts, sighash_type).expect("failed to construct sighash");

        // Note: This will be different from Taproot due to BLAKE3 usage and domain separation
        // The sighash value changes when domain separation tags are added
        // New sighash: [88, 137, 218, 255, 146, 204, 168, 23, 214, 176, 155, 144, 77, 246, 202, 16, 62, 125, 227, 85, 205, 242, 27, 41, 79, 112, 157, 95, 6, 11, 39, 85]

        let msg = Message::from(sighash);
        let signature = secp.sign_schnorr(&msg, &keypair);
        let witness = CopperootWitness::p2cr_key_spend(signature, match sighash_type {
            CopperootSighashType::Default => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::Default,
            CopperootSighashType::All => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::All,
            CopperootSighashType::None => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::None,
            CopperootSighashType::Single => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::Single,
            CopperootSighashType::AllPlusAnyoneCanPay => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::AllPlusAnyoneCanPay,
            CopperootSighashType::NonePlusAnyoneCanPay => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::NonePlusAnyoneCanPay,
            CopperootSighashType::SinglePlusAnyoneCanPay => tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType::SinglePlusAnyoneCanPay,
        });
        tx.inputs[input_index].signature_script = (&witness).try_into()
            .expect("Valid witness conversion");

        let entry = UtxoEntry {
            amount: 100,
            script_public_key: ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, script_pub_key.clone()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        // Direct Copperoot verification (bypass ScriptClass detection since
        // Taproot and Copperoot share identical ScriptPubKey format)
        let witness = CopperootWitness::try_from(tx.inputs[input_index].signature_script.as_slice())
            .expect("Failed to parse Copperoot witness");
        
        // Extract and verify key spend signature
        let sig_bytes = CopperootVariant::extract_key_spend_signature(&witness)
            .expect("Failed to extract key spend signature");
        
        let secp = Secp256k1::new();
        let sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes)
            .expect("Invalid signature format");
        
        let populated_tx = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        let msg = CopperootVariant::key_spend_sighash(&populated_tx, input_index)
            .expect("Failed to compute sighash");
        
        secp.verify_schnorr(&sig, &msg, &xonly_pubkey)
            .expect("Signature verification failed");
    }

    #[test]
    fn test_copperoot_script_spend() {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            &hex::decode("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160")
                .expect("Valid hex string"),
        )
        .expect("Valid private key");
        let internal_key = keypair.x_only_public_key().0;

        // Simple test: single script (leaf), no merkle tree
        let leaf_script = vec![0x51]; // OP_TRUE
        let leaf_version = 0x00;
        
        // Compute leaf hash using BLAKE3
        let leaf_hash = compute_copperoot_leaf_hash(&leaf_script, leaf_version);
        
        // No merkle path for single script
        let merkle_path = vec![];
        let merkle_root = leaf_hash;
        
        // Compute Copperoot Tweak
        let mut tweak_hasher = Hasher::new();
        tweak_hasher.update(b"CopperTweak");
        tweak_hasher.update(&internal_key.serialize());
        tweak_hasher.update(&merkle_root);
        tweak_hasher.update(&[0u8]); // proof_type = 0 (Merkle)
        let tweak_bytes = *tweak_hasher.finalize().as_bytes();
        
        let tweak_scalar = secp256k1::Scalar::from_be_bytes(tweak_bytes)
            .expect("Valid tweak scalar");
        let (tweaked_xonly, parity) = internal_key.add_tweak(&secp, &tweak_scalar)
            .expect("Valid tweak operation");
        
        // Encode parity in parity_leaf_version: bit7 = parity, low 7 bits = leaf_version
        let parity_bit = matches!(parity, secp256k1::Parity::Odd);
        let parity_leaf_version = if parity_bit { 0x80 | leaf_version } else { leaf_version };
        
        let ctrl_block = CopperootControlBlock::new_merkle(parity_leaf_version, internal_key, merkle_path)
            .expect("Valid control block");
        
        let script_pub_key = SmallVec::from_iter([OpTrue, OpData32].into_iter().chain(tweaked_xonly.serialize()));

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3")
            .expect("Valid transaction ID");

        let mut tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::new(SCRIPT_VER_P2CR, script_pub_key.clone()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let input_index = 0;

        let mut witness = BtcWitness::new();
        witness.push(leaf_script);
        witness.push(ctrl_block.serialize());

        tx.inputs[input_index].signature_script = (&CopperootWitness::from(witness)).try_into()
            .expect("Valid witness conversion");

        let entry = UtxoEntry {
            amount: 100,
            script_public_key: ScriptPublicKey::new(SCRIPT_VER_P2CR, script_pub_key.clone()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let reused_values = SigHashReusedValuesUnsync::new();
        let cache = Cache::new(10_000);
        let populated_tx = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        let mut engine = TxScriptEngine::from_transaction_input(
            &populated_tx,
            &tx.inputs[input_index],
            input_index,
            &entry,
            &reused_values,
            &cache,
            false,
            false,
        );
        let result = engine.execute();
        if let Err(e) = result {
            panic!("Engine execution failed: {:?}", e);
        }
    }
}
