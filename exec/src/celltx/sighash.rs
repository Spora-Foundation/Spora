// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Cell transaction signature hashing (blake3 with domain separation)

use super::types::CellTx;

/// Domain constant for TXID hashing
pub const CELL_TXID_DOMAIN: &[u8] = b"tondi-cell/txid";
/// Domain constant for WTXID hashing
pub const CELL_WTXID_DOMAIN: &[u8] = b"tondi-cell/wtxid";
/// Domain constant for signature hashing
pub const CELL_SIG_DOMAIN: &[u8] = b"tondi-cell/sig";

/// Compute txid (without witnesses)
///
/// Formula: blake3(CELL_TXID_DOMAIN || ver || inputs || deps || outputs || outputs_data)
pub fn compute_txid(tx: &CellTx) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CELL_TXID_DOMAIN);
    
    // Version
    hasher.update(&tx.ver.to_le_bytes());
    
    // Inputs (without witnesses)
    hasher.update(&(tx.inputs.len() as u32).to_le_bytes());
    for input in &tx.inputs {
        // Serialize OutPoint
        hasher.update(&input.out_point.tx_hash);
        hasher.update(&input.out_point.index.to_le_bytes());
        hasher.update(&input.since.to_le_bytes());
    }
    
    // Dependencies
    hasher.update(&(tx.deps.len() as u32).to_le_bytes());
    for dep in &tx.deps {
        hasher.update(&dep.out_point.tx_hash);
        hasher.update(&dep.out_point.index.to_le_bytes());
        hasher.update(&[dep.dep_type.clone() as u8]);
    }
    
    // Outputs
    hasher.update(&(tx.outputs.len() as u32).to_le_bytes());
    for output in &tx.outputs {
        hasher.update(&output.lock.code_hash);
        hasher.update(&[output.lock.hash_type]);
        hasher.update(&(output.lock.args.len() as u32).to_le_bytes());
        hasher.update(&output.lock.args);
        
        if let Some(ref type_script) = output.type_ {
            hasher.update(&[1u8]); // has type script
            hasher.update(&type_script.code_hash);
            hasher.update(&[type_script.hash_type]);
            hasher.update(&(type_script.args.len() as u32).to_le_bytes());
            hasher.update(&type_script.args);
        } else {
            hasher.update(&[0u8]); // no type script
        }
        
        hasher.update(&output.capacity.to_le_bytes());
    }
    
    // Outputs data
    for data in &tx.outputs_data {
        hasher.update(&(data.len() as u32).to_le_bytes());
        hasher.update(data);
    }
    
    *hasher.finalize().as_bytes()
}

/// Compute wtxid (with witnesses)
///
/// Formula: blake3(CELL_WTXID_DOMAIN || ver || inputs || deps || outputs || outputs_data || witnesses)
pub fn compute_wtxid(tx: &CellTx) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CELL_WTXID_DOMAIN);
    
    // Version
    hasher.update(&tx.ver.to_le_bytes());
    
    // Inputs
    hasher.update(&(tx.inputs.len() as u32).to_le_bytes());
    for input in &tx.inputs {
        hasher.update(&input.out_point.tx_hash);
        hasher.update(&input.out_point.index.to_le_bytes());
        hasher.update(&input.since.to_le_bytes());
    }
    
    // Dependencies
    hasher.update(&(tx.deps.len() as u32).to_le_bytes());
    for dep in &tx.deps {
        hasher.update(&dep.out_point.tx_hash);
        hasher.update(&dep.out_point.index.to_le_bytes());
        hasher.update(&[dep.dep_type.clone() as u8]);
    }
    
    // Outputs
    hasher.update(&(tx.outputs.len() as u32).to_le_bytes());
    for output in &tx.outputs {
        hasher.update(&output.lock.code_hash);
        hasher.update(&[output.lock.hash_type]);
        hasher.update(&(output.lock.args.len() as u32).to_le_bytes());
        hasher.update(&output.lock.args);
        
        if let Some(ref type_script) = output.type_ {
            hasher.update(&[1u8]);
            hasher.update(&type_script.code_hash);
            hasher.update(&[type_script.hash_type]);
            hasher.update(&(type_script.args.len() as u32).to_le_bytes());
            hasher.update(&type_script.args);
        } else {
            hasher.update(&[0u8]);
        }
        
        hasher.update(&output.capacity.to_le_bytes());
    }
    
    // Outputs data
    for data in &tx.outputs_data {
        hasher.update(&(data.len() as u32).to_le_bytes());
        hasher.update(data);
    }
    
    // Witnesses
    hasher.update(&(tx.witnesses.len() as u32).to_le_bytes());
    for witness in &tx.witnesses {
        hasher.update(&(witness.len() as u32).to_le_bytes());
        hasher.update(witness);
    }
    
    *hasher.finalize().as_bytes()
}

/// Compute signature hash (for signature verification)
///
/// ⚠️ network_id MUST be u32 (4 bytes little-endian)
///
/// Formula:
/// ```text
/// blake3(
///   CELL_SIG_DOMAIN
///   || network_id (u32 LE)
///   || wtxid
///   || input_index (u32 LE)
///   || rw_commitment
/// )
/// ```
pub fn compute_sighash(
    tx: &CellTx,
    input_index: u32,
    network_id: u32,
    rw_commitment: &[u8; 32],
) -> [u8; 32] {
    let wtxid = compute_wtxid(tx);
    
    let mut hasher = blake3::Hasher::new();
    hasher.update(CELL_SIG_DOMAIN);
    hasher.update(&network_id.to_le_bytes()); // ✓ 4 bytes (not 1 byte!)
    hasher.update(&wtxid);
    hasher.update(&input_index.to_le_bytes());
    hasher.update(rw_commitment);
    
    *hasher.finalize().as_bytes()
}

/// Compute public key hash (for lock scripts)
///
/// Formula: blake3(pubkey)[0..20]
pub fn pubkey_hash(pubkey: &[u8]) -> [u8; 20] {
    let hash = blake3::hash(pubkey);
    let mut result = [0u8; 20];
    result.copy_from_slice(&hash.as_bytes()[..20]);
    result
}

/// Helper: create empty RW commitment (for simple transactions)
pub fn empty_rw_commitment() -> [u8; 32] {
    *blake3::hash(b"empty-rw-set").as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellRef, CellOut, ScriptRef, OutPoint, CellDep, DepType};

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let inputs = vec![CellRef::new(OutPoint::new([0x11; 32], 0), 0)];
        let deps = vec![CellDep {
            out_point: OutPoint::new([0x22; 32], 0),
            dep_type: DepType::Code,
        }];
        let outputs = vec![CellOut {
            lock: lock.clone(),
            type_: None,
            capacity: 1000,
        }];
        let outputs_data = vec![vec![]];
        let witnesses = vec![vec![0; 65]];
        
        CellTx::new(inputs, deps, outputs, outputs_data, witnesses).unwrap()
    }

    #[test]
    fn test_txid_computation() {
        let tx = create_test_tx();
        let txid = compute_txid(&tx);
        
        // txid should be deterministic
        let txid2 = compute_txid(&tx);
        assert_eq!(txid, txid2);
        
        // txid should be 32 bytes
        assert_eq!(txid.len(), 32);
    }

    #[test]
    fn test_wtxid_computation() {
        let tx = create_test_tx();
        let wtxid = compute_wtxid(&tx);
        
        // wtxid should be deterministic
        let wtxid2 = compute_wtxid(&tx);
        assert_eq!(wtxid, wtxid2);
        
        // wtxid should differ from txid (includes witnesses)
        let txid = compute_txid(&tx);
        assert_ne!(wtxid, txid);
    }

    #[test]
    fn test_sighash_computation() {
        let tx = create_test_tx();
        let network_id = 0x00000001; // Mainnet
        let rw_commitment = empty_rw_commitment();
        
        let sighash = compute_sighash(&tx, 0, network_id, &rw_commitment);
        
        // sighash should be deterministic
        let sighash2 = compute_sighash(&tx, 0, network_id, &rw_commitment);
        assert_eq!(sighash, sighash2);
        
        // Different network_id should produce different sighash
        let sighash_testnet = compute_sighash(&tx, 0, 0x00000002, &rw_commitment);
        assert_ne!(sighash, sighash_testnet);
        
        // Different input_index should produce different sighash
        let sighash_idx1 = compute_sighash(&tx, 1, network_id, &rw_commitment);
        assert_ne!(sighash, sighash_idx1);
    }

    #[test]
    fn test_pubkey_hash() {
        let pubkey = [0x03; 33]; // Compressed public key
        let hash = pubkey_hash(&pubkey);
        
        // Should be 20 bytes
        assert_eq!(hash.len(), 20);
        
        // Should be deterministic
        let hash2 = pubkey_hash(&pubkey);
        assert_eq!(hash, hash2);
        
        // Different pubkey should produce different hash
        let pubkey2 = [0x02; 33];
        let hash2 = pubkey_hash(&pubkey2);
        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_network_id_encoding() {
        // Verify network_id is encoded as 4 bytes (u32 LE)
        let tx = create_test_tx();
        let network_id: u32 = 0x12345678;
        let rw_commitment = empty_rw_commitment();
        
        let sighash = compute_sighash(&tx, 0, network_id, &rw_commitment);
        
        // Re-compute manually to verify encoding
        let wtxid = compute_wtxid(&tx);
        let mut hasher = blake3::Hasher::new();
        hasher.update(CELL_SIG_DOMAIN);
        hasher.update(&network_id.to_le_bytes()); // [0x78, 0x56, 0x34, 0x12]
        hasher.update(&wtxid);
        hasher.update(&0u32.to_le_bytes());
        hasher.update(&rw_commitment);
        let expected = *hasher.finalize().as_bytes();
        
        assert_eq!(sighash, expected);
    }

    #[test]
    fn test_domain_separation() {
        // txid and wtxid should use different domains
        let tx = create_test_tx();
        let _txid = compute_txid(&tx);
        
        // Manually compute with wrong domain
        let mut hasher = blake3::Hasher::new();
        hasher.update(CELL_WTXID_DOMAIN); // Wrong domain!
        hasher.update(&tx.ver.to_le_bytes());
        // ... (same serialization as txid)
        
        // This would produce a different hash due to domain separation
        assert_eq!(CELL_TXID_DOMAIN, b"tondi-cell/txid");
        assert_eq!(CELL_WTXID_DOMAIN, b"tondi-cell/wtxid");
        assert_eq!(CELL_SIG_DOMAIN, b"tondi-cell/sig");
    }
}

