// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Spora consensus interface (Phase 5)

//! Spora Consensus Layer
//!
//! This crate defines the interface for Spora consensus, which will eventually
//! replace pure GhostDAG scoring with:
//! - **DA weight**: Data availability sampling scores
//! - **Execution weight**: Cell transaction execution costs
//! - **Topology weight**: GhostDAG blue score (preserved)
//!
//! Current status: **Interface only** (actual Spora implementation TBD)

#![warn(missing_docs)]

/// Spora consensus errors
#[derive(Debug, thiserror::Error)]
pub enum SporaError {
    /// Invalid block weight
    #[error("Invalid block weight: {0}")]
    InvalidWeight(String),
    
    /// DA verification failed
    #[error("DA verification failed: {0}")]
    DAVerificationFailed(String),
    
    /// Execution verification failed
    #[error("Execution verification failed: {0}")]
    ExecutionVerificationFailed(String),
}

/// Result type for Spora operations
pub type Result<T> = std::result::Result<T, SporaError>;

/// Spora block weight components
///
/// Weight formula:
/// ```text
/// W = α·DA + β·Exec + γ·Topo
/// ```
/// where:
/// - DA: Data availability score (sampling coverage)
/// - Exec: Execution weight (cycles, gas)
/// - Topo: Topological score (GhostDAG blue score)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockWeight {
    /// Data availability score (0.0 - 1.0)
    pub da_score: f64,
    
    /// Execution weight (normalized cycles)
    pub exec_weight: f64,
    
    /// Topological score (GhostDAG blue score)
    pub topo_score: u64,
    
    /// Combined weight
    pub total_weight: f64,
}

impl BlockWeight {
    /// Create a new block weight
    ///
    /// # Parameters
    /// - `da_score`: DA sampling coverage (0.0 - 1.0)
    /// - `exec_weight`: Execution cost (normalized)
    /// - `topo_score`: GhostDAG blue score
    /// - `alpha`: DA weight coefficient (default: 0.4)
    /// - `beta`: Execution weight coefficient (default: 0.3)
    /// - `gamma`: Topology weight coefficient (default: 0.3)
    pub fn new(
        da_score: f64,
        exec_weight: f64,
        topo_score: u64,
        alpha: f64,
        beta: f64,
        gamma: f64,
    ) -> Self {
        let total_weight = alpha * da_score 
            + beta * exec_weight 
            + gamma * (topo_score as f64);
        
        Self {
            da_score,
            exec_weight,
            topo_score,
            total_weight,
        }
    }
    
    /// Create with default coefficients (α=0.4, β=0.3, γ=0.3)
    pub fn with_defaults(da_score: f64, exec_weight: f64, topo_score: u64) -> Self {
        Self::new(da_score, exec_weight, topo_score, 0.4, 0.3, 0.3)
    }
    
    /// Create from GhostDAG only (fallback mode)
    pub fn from_ghostdag(topo_score: u64) -> Self {
        Self {
            da_score: 1.0, // Assume full DA
            exec_weight: 1.0, // Assume valid execution
            topo_score,
            total_weight: topo_score as f64,
        }
    }
}

/// Spora consensus interface
///
/// Future implementations will provide:
/// - DA sampling verification
/// - Execution proof verification
/// - Weight computation
pub trait SporaConsensus {
    /// Compute block weight
    fn compute_weight(&self, block_meta: &BlockMeta) -> Result<BlockWeight>;
    
    /// Verify DA proof
    fn verify_da(&self, block_meta: &BlockMeta) -> Result<bool>;
    
    /// Verify execution receipts
    fn verify_execution(&self, block_meta: &BlockMeta) -> Result<bool>;
    
    /// Get consensus parameters
    fn params(&self) -> &SporaParams;
}

/// Block metadata (simplified)
///
/// Future: replace with actual consensus types
#[derive(Debug, Clone)]
pub struct BlockMeta {
    /// Block hash
    pub hash: [u8; 32],
    
    /// DAA score (GhostDAG blue score)
    pub daa_score: u64,
    
    /// Number of transactions
    pub tx_count: u32,
    
    /// Total execution cycles
    pub total_cycles: u64,
    
    /// DA commitment (Merkle root)
    pub da_root: [u8; 32],
    
    /// Cell state root
    pub cell_root: [u8; 32],
}

/// Spora consensus parameters
#[derive(Debug, Clone, Copy)]
pub struct SporaParams {
    /// DA weight coefficient (α)
    pub alpha: f64,
    
    /// Execution weight coefficient (β)
    pub beta: f64,
    
    /// Topology weight coefficient (γ)
    pub gamma: f64,
    
    /// Minimum DA sampling rate (0.0 - 1.0)
    pub min_da_rate: f64,
    
    /// Maximum cycles per block
    pub max_cycles: u64,
}

impl Default for SporaParams {
    fn default() -> Self {
        Self {
            alpha: 0.4,
            beta: 0.3,
            gamma: 0.3,
            min_da_rate: 0.5,
            max_cycles: 10_000_000_000, // 10B cycles
        }
    }
}

/// Default Spora implementation (placeholder)
///
/// Currently just wraps GhostDAG, no actual Spora logic
pub struct DefaultSpora {
    params: SporaParams,
}

impl DefaultSpora {
    /// Create a new default Spora instance
    pub fn new(params: SporaParams) -> Self {
        Self { params }
    }
}

impl Default for DefaultSpora {
    fn default() -> Self {
        Self::new(SporaParams::default())
    }
}

impl SporaConsensus for DefaultSpora {
    fn compute_weight(&self, block_meta: &BlockMeta) -> Result<BlockWeight> {
        // Placeholder: assume full DA and valid execution
        let da_score = 1.0;
        let exec_weight = (block_meta.total_cycles as f64) 
            / (self.params.max_cycles as f64);
        
        Ok(BlockWeight::new(
            da_score,
            exec_weight,
            block_meta.daa_score,
            self.params.alpha,
            self.params.beta,
            self.params.gamma,
        ))
    }
    
    fn verify_da(&self, _block_meta: &BlockMeta) -> Result<bool> {
        // Placeholder: always pass
        Ok(true)
    }
    
    fn verify_execution(&self, _block_meta: &BlockMeta) -> Result<bool> {
        // Placeholder: always pass
        Ok(true)
    }
    
    fn params(&self) -> &SporaParams {
        &self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_weight_calculation() {
        let weight = BlockWeight::with_defaults(1.0, 0.5, 100);
        
        // W = 0.4*1.0 + 0.3*0.5 + 0.3*100 = 0.4 + 0.15 + 30 = 30.55
        assert!((weight.total_weight - 30.55).abs() < 0.01);
    }

    #[test]
    fn test_block_weight_from_ghostdag() {
        let weight = BlockWeight::from_ghostdag(200);
        
        assert_eq!(weight.da_score, 1.0);
        assert_eq!(weight.exec_weight, 1.0);
        assert_eq!(weight.topo_score, 200);
        assert_eq!(weight.total_weight, 200.0);
    }

    #[test]
    fn test_default_spora() {
        let spora = DefaultSpora::default();
        
        let block_meta = BlockMeta {
            hash: [0x42; 32],
            daa_score: 100,
            tx_count: 10,
            total_cycles: 1_000_000_000,
            da_root: [0x01; 32],
            cell_root: [0x02; 32],
        };
        
        let weight = spora.compute_weight(&block_meta).unwrap();
        assert!(weight.total_weight > 0.0);
        
        assert!(spora.verify_da(&block_meta).unwrap());
        assert!(spora.verify_execution(&block_meta).unwrap());
    }

    #[test]
    fn test_spora_params() {
        let params = SporaParams::default();
        
        assert_eq!(params.alpha + params.beta + params.gamma, 1.0);
        assert!(params.min_da_rate >= 0.0 && params.min_da_rate <= 1.0);
        assert!(params.max_cycles > 0);
    }
}
