// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load-header-timestamp lock script test

#[cfg(all(test, feature = "vm"))]
mod tests {
    use crate::celltx::{CellOut, CellRef, CellTx, OutPoint, ScriptRef};
    use crate::scripts::{load_header_timestamp_code_hash, LOAD_HEADER_TIMESTAMP_SCRIPT};
    use crate::vm::{ResolvedCell, ResolvedHeader, ScriptVersion, SimpleDataProvider, TransactionScriptVerifier};
    use std::sync::Arc;

    const EXPECTED_TIMESTAMP: u64 = 0x0102_0304_0506_0708;

    fn build_provider(code_hash: [u8; 32], input_out_point: OutPoint, header_hash: [u8; 32], timestamp: u64) -> SimpleDataProvider {
        let mut provider = SimpleDataProvider::new();
        provider.add_script(code_hash, LOAD_HEADER_TIMESTAMP_SCRIPT.to_vec());
        provider.add_cell(
            input_out_point.tx_hash,
            input_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 1000, lock: ScriptRef { code_hash, hash_type: 0, args: vec![] }, type_: None },
                data: Some(vec![]),
            },
        );
        provider.add_header(
            header_hash,
            ResolvedHeader { hash: header_hash, timestamp, daa_score: 42, parents: vec![[0x99; 32], [0xAA; 32]] },
        );
        provider
    }

    #[test]
    fn test_load_header_timestamp_verification() {
        let code_hash = load_header_timestamp_code_hash();
        let input_out_point = OutPoint::new([0x66; 32], 0);
        let header_hash = [0x77; 32];
        let provider = build_provider(code_hash, input_out_point.clone(), header_hash, EXPECTED_TIMESTAMP);
        let tx = CellTx {
            ver: 0xC001,
            inputs: vec![CellRef::new(input_out_point, 0)],
            deps: vec![],
            header_deps: vec![header_hash],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        };

        let verifier =
            TransactionScriptVerifier::new(Arc::new(tx), Arc::new(provider)).with_version(ScriptVersion::V2).with_max_cycles(20_000);

        assert!(verifier.verify().is_ok());
    }

    #[test]
    fn test_load_header_timestamp_rejects_wrong_header() {
        let code_hash = load_header_timestamp_code_hash();
        let input_out_point = OutPoint::new([0x68; 32], 0);
        let header_hash = [0x79; 32];
        let provider = build_provider(code_hash, input_out_point.clone(), header_hash, 7);
        let tx = CellTx {
            ver: 0xC001,
            inputs: vec![CellRef::new(input_out_point, 0)],
            deps: vec![],
            header_deps: vec![header_hash],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        };

        let verifier =
            TransactionScriptVerifier::new(Arc::new(tx), Arc::new(provider)).with_version(ScriptVersion::V2).with_max_cycles(20_000);

        assert!(verifier.verify().is_err());
    }
}
