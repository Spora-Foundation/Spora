use crate::tx::TransactionId;
use spora_hashes::Hash;

/// Returns the canonical transaction hash for a CellTx.
///
/// The transaction ID is computed directly from the CellTx serialization.
pub fn hash(tx: &crate::tx::CellTx) -> Hash {
    TransactionId::from_bytes(tx.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{outpoint_from_id, CellOutput, CellInput, CellTx, Script};
    use spora_hashes::Hash;

    fn sample_non_coinbase_tx() -> CellTx {
        let input = CellInput::new(
            outpoint_from_id(Hash::from_u64_word(0), 2).into(),
            7, // since
        );
        let output = CellOutput { capacity: 1564, lock: Script::new([1u8; 32], 0, vec![1, 2, 3, 4, 5]), type_: None };
        CellTx::new(
            vec![input],
            vec![], // cell_deps
            vec![output],
            vec![vec![]],     // outputs_data
            vec![vec![1, 2]], // witnesses
        )
        .unwrap()
    }

    fn sample_coinbase_tx(payload: Vec<u8>) -> CellTx {
        let output = CellOutput { capacity: 5_000, lock: Script::new([0xaau8; 32], 0, vec![]), type_: None };
        CellTx::new(
            vec![], // no inputs for coinbase
            vec![], // cell_deps
            vec![output],
            vec![payload], // payload in outputs_data[0] for coinbase
            vec![],        // witnesses
        )
        .unwrap()
    }

    #[test]
    fn test_tx_id() {
        let tx = sample_non_coinbase_tx();
        let id = hash(&tx);
        assert_ne!(id, Hash::default());

        // Same transaction should produce same ID
        let tx2 = sample_non_coinbase_tx();
        let id2 = hash(&tx2);
        assert_eq!(id, id2);
    }

    #[test]
    fn test_coinbase_tx_id() {
        let tx = sample_coinbase_tx(vec![1, 2, 3]);
        assert!(tx.is_coinbase());
        let id = hash(&tx);
        assert_ne!(id, Hash::default());
    }
}
