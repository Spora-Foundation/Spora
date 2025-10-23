// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Always-success lock script test

#[cfg(all(test, feature = "vm"))]
mod tests {
    use crate::celltx::{CellTx, CellOut, ScriptRef};
    use crate::vm::{TransactionScriptVerifier, SimpleDataProvider, ScriptVersion};
    use crate::scripts::{ALWAYS_SUCCESS_SCRIPT, always_success_code_hash};
    use std::sync::Arc;

    #[test]
    fn test_always_success_verification() {
        // Create data provider with always-success script
        let mut provider = SimpleDataProvider::new();
        let code_hash = always_success_code_hash();
        provider.add_script(code_hash, ALWAYS_SUCCESS_SCRIPT.to_vec());

        // Create transaction with always-success lock
        let tx = CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
            outputs: vec![
                CellOut {
                    capacity: 1000,
                    lock: ScriptRef {
                        code_hash,
                        hash_type: 0,
                        args: vec![],
                    },
                    type_: None,
                }
            ],
            outputs_data: vec![vec![]],
            witnesses: vec![],
        };

        // Create verifier
        let verifier = TransactionScriptVerifier::new(
            Arc::new(tx),
            Arc::new(provider),
        )
        .with_version(ScriptVersion::V2)
        .with_max_cycles(10_000);

        // Verify should succeed
        let result = verifier.verify();
        assert!(result.is_ok(), "Always-success script should verify: {:?}", result);
    }

    #[test]
    fn test_script_not_found() {
        let provider = SimpleDataProvider::new();
        // Don't add any scripts

        let tx = CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
            outputs: vec![
                CellOut {
                    capacity: 1000,
                    lock: ScriptRef {
                        code_hash: [0xFF; 32],  // Non-existent script
                        hash_type: 0,
                        args: vec![],
                    },
                    type_: None,
                }
            ],
            outputs_data: vec![vec![]],
            witnesses: vec![],
        };

        let verifier = TransactionScriptVerifier::new(
            Arc::new(tx),
            Arc::new(provider),
        );

        // Should fail with ScriptNotFound
        let result = verifier.verify();
        assert!(result.is_err());
    }
}

