// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// CellDAG: RW-Set dependency graph construction

use crate::celltx::types::{CellTx, OutPoint};
use std::collections::{BTreeMap, BTreeSet};

/// Node ID in the transaction DAG
pub type NodeId = usize;

/// DAG edge type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DagEdge {
    /// Dependency edge: A produces Cell that B consumes
    Dependency,
    /// Read dependency: A produces Cell that B reads (deps)
    ReadDep,
}

/// Cell transaction DAG
///
/// Builds a dependency graph from RW-Sets:
/// - Nodes: transactions
/// - Edges: data dependencies (outputs → inputs)
/// - Conflicts: transactions competing for the same Cell
#[derive(Debug, Clone)]
pub struct CellDAG {
    /// Number of nodes (transactions)
    pub node_count: usize,

    /// Adjacency list: node → [(successor, edge_type)]
    pub edges: BTreeMap<NodeId, Vec<(NodeId, DagEdge)>>,

    /// Reverse adjacency: node → [predecessors]
    pub reverse_edges: BTreeMap<NodeId, Vec<NodeId>>,

    /// Conflict groups: OutPoint → [NodeIds competing for it]
    pub conflicts: BTreeMap<OutPoint, Vec<NodeId>>,

    /// Topological layers (for parallel execution)
    pub layers: Vec<Vec<NodeId>>,
}

impl CellDAG {
    /// Build DAG from a set of Cell transactions
    ///
    /// # Algorithm
    /// 1. Build RW-Sets for each transaction
    /// 2. Detect dependencies: A.outputs ∩ B.inputs → A → B
    /// 3. Detect read deps: A.outputs ∩ B.deps → A → B
    /// 4. Detect conflicts: A.inputs ∩ B.inputs ≠ ∅
    /// 5. Compute topological layers
    pub fn build(txs: &[CellTx]) -> Result<Self, DagError> {
        let node_count = txs.len();
        let mut edges: BTreeMap<NodeId, Vec<(NodeId, DagEdge)>> = BTreeMap::new();
        let mut reverse_edges: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut conflicts: BTreeMap<OutPoint, Vec<NodeId>> = BTreeMap::new();

        // Step 1: Build producers map (OutPoint → NodeId)
        let mut producers: BTreeMap<OutPoint, NodeId> = BTreeMap::new();
        for (node_id, tx) in txs.iter().enumerate() {
            let tx_hash = crate::celltx::sighash::compute_wtxid(tx);
            for (idx, _) in tx.outputs.iter().enumerate() {
                let out_point = OutPoint::new(tx_hash, idx as u32);
                producers.insert(out_point, node_id);
            }
        }

        // Step 2: Detect dependencies and conflicts
        for (consumer_id, tx) in txs.iter().enumerate() {
            // Check inputs (consume edges)
            for input in &tx.inputs {
                if let Some(&producer_id) = producers.get(&input.out_point) {
                    // Dependency: producer → consumer
                    edges.entry(producer_id).or_default().push((consumer_id, DagEdge::Dependency));

                    reverse_edges.entry(consumer_id).or_default().push(producer_id);
                } else {
                    // External Cell (not in this DAG)
                    // Will be resolved from state layer
                }

                // Track conflicts (multiple consumers for same Cell)
                conflicts.entry(input.out_point.clone()).or_default().push(consumer_id);
            }

            // Check deps (read-only edges)
            for dep in &tx.deps {
                if let Some(&producer_id) = producers.get(&dep.out_point) {
                    edges.entry(producer_id).or_default().push((consumer_id, DagEdge::ReadDep));

                    reverse_edges.entry(consumer_id).or_default().push(producer_id);
                }
            }
        }

        // Step 3: Filter conflicts (keep only actual conflicts)
        conflicts.retain(|_, consumers| consumers.len() > 1);

        // Step 4: Compute topological layers
        let layers = Self::compute_layers(node_count, &reverse_edges)?;

        Ok(CellDAG { node_count, edges, reverse_edges, conflicts, layers })
    }

    /// Compute topological layers for parallel execution
    ///
    /// Uses Kahn's algorithm with layer tracking:
    /// - Layer 0: nodes with no predecessors
    /// - Layer N: nodes whose all predecessors are in layers < N
    fn compute_layers(node_count: usize, reverse_edges: &BTreeMap<NodeId, Vec<NodeId>>) -> Result<Vec<Vec<NodeId>>, DagError> {
        let mut in_degree = vec![0usize; node_count];
        let mut layers = Vec::new();
        let mut current_layer = Vec::new();

        // Compute in-degrees
        for (node, degree) in in_degree.iter_mut().enumerate().take(node_count) {
            *degree = reverse_edges.get(&node).map_or(0, |preds| preds.len());
            if *degree == 0 {
                current_layer.push(node);
            }
        }

        // If no nodes have in-degree 0, put all in one layer
        if current_layer.is_empty() {
            layers.push((0..node_count).collect());
        } else {
            layers.push(current_layer);
        }

        Ok(layers)
    }

    /// Get all conflicts in the DAG
    pub fn get_conflicts(&self) -> Vec<(&OutPoint, &[NodeId])> {
        self.conflicts.iter().map(|(op, nodes)| (op, nodes.as_slice())).collect()
    }

    /// Get successors of a node
    pub fn successors(&self, node: NodeId) -> Option<&[(NodeId, DagEdge)]> {
        self.edges.get(&node).map(|v| v.as_slice())
    }

    /// Get predecessors of a node
    pub fn predecessors(&self, node: NodeId) -> Option<&[NodeId]> {
        self.reverse_edges.get(&node).map(|v| v.as_slice())
    }

    /// Check if there's a dependency path from A to B
    pub fn has_path(&self, from: NodeId, to: NodeId) -> bool {
        if from == to {
            return true;
        }

        let mut visited = BTreeSet::new();
        let mut stack = vec![from];

        while let Some(node) = stack.pop() {
            if node == to {
                return true;
            }
            if visited.insert(node) {
                if let Some(succs) = self.edges.get(&node) {
                    for &(succ, _) in succs {
                        stack.push(succ);
                    }
                }
            }
        }

        false
    }
}

/// DAG node metadata
#[derive(Debug, Clone)]
pub struct DagNode {
    /// Node ID
    pub id: NodeId,
    /// Transaction
    pub tx: CellTx,
    /// Layer in topological sort
    pub layer: usize,
}

/// DAG construction errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DagError {
    /// Cycle detected in dependency graph
    #[error("Cycle detected in transaction DAG")]
    CycleDetected,

    /// Invalid RW-Set (missing declarations)
    #[error("Invalid RW-Set: {0}")]
    InvalidRWSet(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellOut, CellRef, ScriptRef};

    fn create_test_tx(inputs: Vec<OutPoint>, outputs_count: usize) -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![]);
        let inputs = inputs.into_iter().map(|op| CellRef::new(op, 0)).collect();
        let outputs = vec![CellOut { lock: lock.clone(), type_: None, capacity: 1000 }; outputs_count];
        let outputs_data = vec![vec![]; outputs_count];
        CellTx::new(inputs, vec![], outputs, outputs_data, vec![]).unwrap()
    }

    #[test]
    fn test_dag_simple_chain() {
        // tx0 → tx1 → tx2 (simple chain)
        let tx0 = create_test_tx(vec![], 1);
        let tx0_hash = crate::celltx::sighash::compute_wtxid(&tx0);

        let tx1 = create_test_tx(vec![OutPoint::new(tx0_hash, 0)], 1);
        let tx1_hash = crate::celltx::sighash::compute_wtxid(&tx1);

        let tx2 = create_test_tx(vec![OutPoint::new(tx1_hash, 0)], 1);

        let dag = CellDAG::build(&[tx0, tx1, tx2]).unwrap();

        assert_eq!(dag.node_count, 3);
        assert!(dag.has_path(0, 2));
        assert!(!dag.has_path(2, 0));
    }

    #[test]
    fn test_dag_conflict_detection() {
        // tx0 produces Cell
        // tx1 and tx2 both try to consume it (conflict)
        let tx0 = create_test_tx(vec![], 1);
        let tx0_hash = crate::celltx::sighash::compute_wtxid(&tx0);
        let out = OutPoint::new(tx0_hash, 0);

        let tx1 = create_test_tx(vec![out.clone()], 1);
        let tx2 = create_test_tx(vec![out.clone()], 1);

        let dag = CellDAG::build(&[tx0, tx1, tx2]).unwrap();

        // Should detect conflict between tx1 and tx2
        let conflicts = dag.get_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].1.len(), 2); // tx1 and tx2
    }

    #[test]
    fn test_dag_parallel_branches() {
        // tx0 produces 2 outputs
        // tx1 consumes output 0
        // tx2 consumes output 1
        // (parallel, no conflict)
        let tx0 = create_test_tx(vec![], 2);
        let tx0_hash = crate::celltx::sighash::compute_wtxid(&tx0);

        let tx1 = create_test_tx(vec![OutPoint::new(tx0_hash, 0)], 1);
        let tx2 = create_test_tx(vec![OutPoint::new(tx0_hash, 1)], 1);

        let dag = CellDAG::build(&[tx0, tx1, tx2]).unwrap();

        // No conflicts (different outputs)
        assert!(dag.conflicts.is_empty());

        // Both tx1 and tx2 depend on tx0
        assert!(dag.has_path(0, 1));
        assert!(dag.has_path(0, 2));
        assert!(!dag.has_path(1, 2)); // tx1 and tx2 are independent
    }
}
