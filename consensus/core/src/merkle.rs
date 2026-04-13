use crate::tx::CellTx;
use spora_hashes::Hash;
use spora_merkle::calc_merkle_root;

/// Calculate merkle root for CellTx transactions
pub fn calc_hash_merkle_root_cell<'a>(txs: impl ExactSizeIterator<Item = &'a CellTx>, _include_mass_field: bool) -> Hash {
    // CellTx.id() already returns the correct transaction hash
    calc_merkle_root(txs.map(|tx| tx.id().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::calc_hash_merkle_root_cell;
    use crate::tx::{outpoint_from_id, CellOutput, CellInput, CellTx, Script, TransactionId};

    fn sample_coinbase_tx() -> CellTx {
        let output =
            CellOutput { capacity: 0x12a05f200, lock: Script::new([0xa9u8; 32], 0, vec![0x14, 0xda, 0x17, 0x45]), type_: None };
        CellTx::new(
            vec![], // no inputs for coinbase
            vec![], // cell_deps
            vec![output],
            vec![vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]], // payload in outputs_data
            vec![],                                                              // witnesses
        )
        .unwrap()
    }

    fn sample_tx_with_inputs() -> CellTx {
        let input1 = CellInput::new(
            outpoint_from_id(
                TransactionId::from_slice(&[
                    0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11, 0x89,
                    0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                ]),
                0xffffffff,
            ),
            u64::MAX, // since
        );
        let input2 = CellInput::new(
            outpoint_from_id(
                TransactionId::from_slice(&[
                    0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04, 0x71,
                    0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                ]),
                0xffffffff,
            ),
            u64::MAX, // since
        );
        CellTx::new(
            vec![input1, input2],
            vec![],               // cell_deps
            vec![],               // outputs
            vec![],               // outputs_data
            vec![vec![], vec![]], // witnesses
        )
        .unwrap()
    }

    fn sample_tx_with_outputs() -> CellTx {
        let input = CellInput::new(
            outpoint_from_id(
                TransactionId::from_slice(&[
                    0x03, 0x2e, 0x38, 0xe9, 0xc0, 0xa8, 0x4c, 0x60, 0x46, 0xd6, 0x87, 0xd1, 0x05, 0x56, 0xdc, 0xac, 0xc4, 0x1d, 0x27,
                    0x5e, 0xc5, 0x5f, 0xc0, 0x07, 0x79, 0xac, 0x88, 0xfd, 0xf3, 0x57, 0xa1, 0x87,
                ]),
                0,
            ),
            u64::MAX, // since
        );
        let output1 = CellOutput { capacity: 0x2123e300, lock: Script::new([0x76u8; 32], 0, vec![0xa9, 0x14]), type_: None };
        let output2 = CellOutput { capacity: 0x108e20f00, lock: Script::new([0x94u8; 32], 0, vec![0xa9, 0x14]), type_: None };
        CellTx::new(
            vec![input],
            vec![], // cell_deps
            vec![output1, output2],
            vec![vec![], vec![]], // outputs_data
            vec![vec![]],         // witnesses
        )
        .unwrap()
    }

    #[test]
    fn merkle_root_test() {
        let txs = vec![sample_coinbase_tx(), sample_tx_with_inputs(), sample_tx_with_outputs()];

        let root = calc_hash_merkle_root_cell(txs.iter(), false);
        assert_ne!(root, Hash::default());

        // Same transactions should produce same root
        let root2 = calc_hash_merkle_root_cell(txs.iter(), false);
        assert_eq!(root, root2);
    }

    #[test]
    fn merkle_root_single_tx() {
        let tx = sample_coinbase_tx();
        let root = calc_hash_merkle_root_cell(std::iter::once(&tx), false);
        assert_eq!(root, Hash::from_bytes(tx.id()));
    }
}
