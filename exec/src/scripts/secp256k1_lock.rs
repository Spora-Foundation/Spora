// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Secp256k1 lock script
// ⚠️ Modified: Blake2b → Blake3

use crate::celltx::sighash::compute_sighash;
use crate::celltx::types::{CellTx, Script};
use secp256k1::{ecdsa::RecoverableSignature, Message, Secp256k1};

/// Secp256k1 lock script error
#[derive(Debug, thiserror::Error)]
pub enum Secp256k1LockError {
    /// Invalid script arguments
    #[error("Invalid script arguments: expected 20 bytes pubkey hash, got {0}")]
    InvalidArgs(usize),
    
    /// Missing witness
    #[error("Missing witness for input {0}")]
    MissingWitness(usize),
    
    /// Invalid witness
    #[error("Invalid witness: expected 65 bytes signature, got {0}")]
    InvalidWitness(usize),
    
    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    
    /// Secp256k1 error
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(String),
}

/// Verify secp256k1 lock script
///
/// Script args: 20 bytes pubkey hash (blake3(pubkey)[0..20])
/// Witness: 65 bytes signature (r + s + recovery_id)
///
/// ⚠️ Uses Blake3 instead of Blake2b (CKB uses Blake2b)
pub fn verify_secp256k1_lock(
    script: &Script,
    tx: &CellTx,
    input_index: usize,
    network_id: u32,
) -> Result<(), Secp256k1LockError> {
    // 1. Extract pubkey hash from script args (20 bytes)
    if script.args.len() != 20 {
        return Err(Secp256k1LockError::InvalidArgs(script.args.len()));
    }
    let expected_pubkey_hash = &script.args[..20];
    
    // 2. Extract signature from witness (65 bytes)
    let witness = tx.witnesses.get(input_index)
        .ok_or(Secp256k1LockError::MissingWitness(input_index))?;
    
    if witness.len() < 65 {
        return Err(Secp256k1LockError::InvalidWitness(witness.len()));
    }
    let signature_bytes = &witness[..65];
    
    // 3. Compute sighash (using Blake3)
    let rw_commitment = [0u8; 32]; // TODO: Compute actual RW-Set commitment
    let sighash = compute_sighash(tx, input_index as u32, network_id, &rw_commitment);
    
    // 4. Recover public key from signature
    let secp = Secp256k1::new();
    
    // Parse recoverable signature (r + s + recovery_id)
    let recovery_id = secp256k1::ecdsa::RecoveryId::from_i32((signature_bytes[64] % 4) as i32)
        .map_err(|e| Secp256k1LockError::Secp256k1Error(e.to_string()))?;
    
    let signature = RecoverableSignature::from_compact(&signature_bytes[..64], recovery_id)
        .map_err(|e| Secp256k1LockError::Secp256k1Error(e.to_string()))?;
    
    let message = Message::from_digest_slice(&sighash)
        .map_err(|e| Secp256k1LockError::Secp256k1Error(e.to_string()))?;
    
    let pubkey = secp.recover_ecdsa(&message, &signature)
        .map_err(|e| Secp256k1LockError::Secp256k1Error(e.to_string()))?;
    
    // 5. Verify pubkey hash (using Blake3, not Blake2b like CKB)
    let pubkey_bytes = pubkey.serialize();
    let computed_hash = blake3::hash(&pubkey_bytes);
    let computed_pubkey_hash = &computed_hash.as_bytes()[..20];
    
    if computed_pubkey_hash != expected_pubkey_hash {
        return Err(Secp256k1LockError::SignatureVerificationFailed);
    }
    
    Ok(())
}

/// Compute pubkey hash (Blake3)
///
/// ⚠️ CKB uses Blake2b, we use Blake3
pub fn pubkey_hash(pubkey: &[u8]) -> [u8; 20] {
    let hash = blake3::hash(pubkey);
    hash.as_bytes()[..20].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellInput, CellOutput, OutPoint};
    use secp256k1::{SecretKey, PublicKey};

    #[test]
    fn test_pubkey_hash() {
        let pubkey = vec![0x02; 33]; // Compressed pubkey
        let hash = pubkey_hash(&pubkey);
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn test_secp256k1_lock_invalid_args() {
        let script = Script::new([0; 32], 0, vec![1, 2, 3]); // Wrong length
        let lock = Script::new([0; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![],
        ).unwrap();
        
        let result = verify_secp256k1_lock(&script, &tx, 0, 0x00000001);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Secp256k1LockError::InvalidArgs(_)));
    }

    #[test]
    fn test_secp256k1_lock_missing_witness() {
        let script = Script::new([0; 32], 0, vec![0; 20]);
        let lock = Script::new([0; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![], // No witnesses
        ).unwrap();
        
        let result = verify_secp256k1_lock(&script, &tx, 0, 0x00000001);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Secp256k1LockError::MissingWitness(_)));
    }

    #[test]
    fn test_secp256k1_full_cycle() {
        // Create a private key
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0x01; 32]).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        // Compute pubkey hash
        let pubkey_bytes = public_key.serialize();
        let pubkey_hash = pubkey_hash(&pubkey_bytes);
        
        // Create script with pubkey hash
        let _script = Script::new([0; 32], 0, pubkey_hash.to_vec());
        
        // Create transaction
        let lock = Script::new([0; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![vec![0; 65]], // Placeholder witness
        ).unwrap();
        
        // TODO: Sign transaction and verify
        // For now, just test the structure
        assert_eq!(tx.witnesses.len(), 1);
        assert_eq!(tx.witnesses[0].len(), 65);
    }
}

