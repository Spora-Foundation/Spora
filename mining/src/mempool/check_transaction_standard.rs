use crate::mempool::{
    errors::{NonStandardError, NonStandardResult},
    Mempool,
};
use spora_consensus_core::mass::NonContextualMasses;
use spora_consensus_core::{
    constants::MAX_SAU,
    tx::{CellOutput, MutableTransaction},
};

/// MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE is the maximum size allowed for a
/// transaction input signature script to be considered standard. This
/// value allows for a 15-of-15 CHECKMULTISIG pay-to-script-hash with
/// compressed keys.
///
/// The form of the overall script is: OP_0 <15 signatures> OP_PUSHDATA2
/// <2 bytes len> [OP_15 <15 pubkeys> OP_15 OP_CHECKMULTISIG]
///
/// For the p2sh script portion, each of the 15 compressed pubkeys are
/// 33 bytes (plus one for the OP_DATA_33 opcode), and the thus it totals
/// to (15*34)+3 = 513 bytes. Next, each of the 15 signatures is a max
/// of 73 bytes (plus one for the OP_DATA_73 opcode). Also, there is one
/// extra byte for the initial extra OP_0 push and 3 bytes for the
/// OP_PUSHDATA2 needed to specify the 513 bytes for the script push.
/// That brings the total to 1+(15*74)+3+513 = 1627. This value also
/// adds a few extra bytes to provide a little buffer.
/// (1 + 15*74 + 3) + (15*34 + 3) + 23 = 1650
const MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE: u64 = 1650;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining.
const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 100_000;

impl Mempool {
    pub(crate) fn check_transaction_standard_in_isolation(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // The transaction must be a currently supported version.
        //
        // This check is currently mirrored in consensus.
        // However, in a later version of Spora the consensus-valid transaction version range might diverge from the
        // standard transaction version range, and thus the validation should happen in both levels.
        if transaction.tx.version() > self.config.maximum_standard_transaction_version
            || transaction.tx.version() < self.config.minimum_standard_transaction_version
        {
            return Err(NonStandardError::RejectVersion(
                transaction_id,
                transaction.tx.version(),
                self.config.minimum_standard_transaction_version,
                self.config.maximum_standard_transaction_version,
            ));
        }

        // Since extremely large transactions with a lot of inputs can cost
        // almost as much to process as the sender fees, limit the maximum
        // size of a transaction. This also helps mitigate CPU exhaustion
        // attacks.
        let NonContextualMasses { compute_mass, transient_mass } = transaction.calculated_non_contextual_masses.unwrap();
        if compute_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectComputeMass(transaction_id, compute_mass, MAXIMUM_STANDARD_TRANSACTION_MASS));
        }
        if transient_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectTransientMass(transaction_id, transient_mass, MAXIMUM_STANDARD_TRANSACTION_MASS));
        }

        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            // Each transaction input witness must not exceed the
            // maximum size allowed for a standard transaction.
            //
            // See the comment on MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE for
            // more details.
            let _ = input; // input used for iteration only
            let witness = transaction.tx.witnesses.get(i).map(|w| w.len()).unwrap_or(0) as u64;
            if witness > MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE {
                return Err(NonStandardError::RejectSignatureScriptSize(
                    transaction_id,
                    i,
                    witness,
                    MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE,
                ));
            }
        }

        // None of the output lock scripts can be a non-standard script or be "dust".
        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            // Standard relay currently only supports CKB-compatible Data-hash lock scripts.
            if output.lock.hash_type != 0 {
                return Err(NonStandardError::RejectLockScriptHashType(transaction_id, i));
            }

            // ScriptClass check removed - all scripts are validated through CKB-VM
            // TODO: Add proper script validation for Cell model

            if self.is_transaction_output_dust_cell(output) {
                return Err(NonStandardError::RejectDust(transaction_id, i, output.capacity));
            }
        }

        Ok(())
    }

    /// is_transaction_output_dust_cell returns whether or not the passed CellOutput
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    pub(crate) fn is_transaction_output_dust_cell(&self, output: &CellOutput) -> bool {
        // Unspendable outputs are considered dust.
        let lock_bytes = output.lock.to_bytes();
        // TODO: Add unspendable script detection for Cell model
        if lock_bytes.is_empty() || lock_bytes[0] == 0x6a {
            // OP_RETURN
            return true;
        }

        // Estimate serialized size for dust calculation
        let output_size: u64 = 8 /* capacity */ + 2 /* script version */ + 8 /* script len */ + lock_bytes.len() as u64;
        let total_serialized_size = output_size + 148;

        match output.capacity.checked_mul(1000) {
            Some(value_1000) => value_1000 / (3 * total_serialized_size) < self.config.minimum_relay_transaction_fee,
            None => {
                (output.capacity as u128 * 1000 / (3 * total_serialized_size as u128))
                    < self.config.minimum_relay_transaction_fee as u128
            }
        }
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Note: This function operates on `CellOutput`.
    #[cfg(test)]
    pub(crate) fn is_transaction_output_dust(&self, transaction_output: &CellOutput) -> bool {
        // Use the Cell model version
        self.is_transaction_output_dust_cell(transaction_output)
    }

    /// check_transaction_standard_in_context performs a series of checks on a transaction's
    /// inputs to ensure they are "standard". A standard transaction input within the
    /// context of this function is one whose referenced public key script is of a
    /// standard form and, for pay-to-script-hash, does not have more than
    /// maxStandardP2SHSigOps signature operations.
    /// In addition, makes sure that the transaction's fee is above the minimum for acceptance
    /// into the mempool and relay.
    pub(crate) fn check_transaction_standard_in_context(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();
        let contextual_storage_mass = transaction
            .contextual_storage_mass()
            .expect("contextual storage mass should be populated before in-context standard checks");
        if contextual_storage_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectStorageMass(
                transaction_id,
                contextual_storage_mass,
                MAXIMUM_STANDARD_TRANSACTION_MASS,
            ));
        }

        // Fee admission should use the same one-dimensional selection mass that mempool
        // ordering and block-template construction use, otherwise high-storage-mass
        // transactions are underpriced at relay time.
        let minimum_fee = self.minimum_required_transaction_relay_fee(
            transaction.selection_mass().expect("selection mass should be populated before in-context standard checks"),
        );
        if transaction.calculated_fee.unwrap() < minimum_fee {
            return Err(NonStandardError::RejectInsufficientFee(transaction_id, transaction.calculated_fee.unwrap(), minimum_fee));
        }

        // Script-class inspection has been removed. Input standardness now relies on:
        // 1. canonical Cell metadata being resolved by the validation pipeline, and
        // 2. CKB-VM script execution enforcing the actual lock/type semantics.
        let _ = transaction;

        Ok(())
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    fn minimum_required_transaction_relay_fee(&self, mass: u64) -> u64 {
        // Calculate the minimum fee for a transaction to be allowed into the
        // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
        // sau/kg so multiply by mass (which is in grams) and divide by 1000 to get
        // minimum saus.
        let mut minimum_fee = (mass * self.config.minimum_relay_transaction_fee) / 1000;

        if minimum_fee == 0 {
            minimum_fee = self.config.minimum_relay_transaction_fee;
        }

        // Set the minimum fee to the maximum possible value if the calculated
        // fee is not in the valid range for monetary amounts.
        minimum_fee = minimum_fee.min(MAX_SAU);

        minimum_fee
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mempool::config::{Config, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE},
        MiningCounters,
    };
    use spora_addresses::{Address, Prefix, Version};
    use spora_consensus_core::{
        cell_diff::CellMeta,
        cell_metadata::CellMetadata,
        config::params::Params,
        constants::{CELL_TX_VERSION, MAX_TX_IN_SEQUENCE_NUM, SAU_PER_SPORA},
        mass::{ContextualMasses, NonContextualMasses},
        network::NetworkType,
        tx::{pay_to_address_lock_script, CellOutput, CellInput, CellTx, MutableTransaction, Script, TransactionOutpoint},
    };
    use std::sync::Arc;

    const OP_RETURN: u8 = 0x6a;
    const OP_TRUE: u8 = 0x51;

    fn lock_script(script: impl Into<Vec<u8>>) -> Script {
        Script::new([0; 32], 0, script.into())
    }

    #[test]
    fn test_calc_min_required_tx_relay_fee() {
        struct Test {
            name: &'static str,
            size: u64,
            minimum_relay_transaction_fee: u64,
            want: u64,
        }

        let tests = vec![
            Test {
                // Ensure combination of size and fee that are less than 1000
                // produce a non-zero fee.
                name: "250 bytes with relay fee of 3",
                size: 250,
                minimum_relay_transaction_fee: 3,
                want: 3,
            },
            Test {
                name: "100 bytes with default minimum relay fee",
                size: 100,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: 100,
            },
            Test {
                name: "max standard tx size with default minimum relay fee",
                size: MAXIMUM_STANDARD_TRANSACTION_MASS,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: 100000,
            },
            Test { name: "1500 bytes with 5000 relay fee", size: 1500, minimum_relay_transaction_fee: 5000, want: 7500 },
            Test { name: "1500 bytes with 3000 relay fee", size: 1500, minimum_relay_transaction_fee: 3000, want: 4500 },
            Test { name: "782 bytes with 5000 relay fee", size: 782, minimum_relay_transaction_fee: 5000, want: 3910 },
            Test { name: "782 bytes with 3000 relay fee", size: 782, minimum_relay_transaction_fee: 3000, want: 2346 },
            Test { name: "782 bytes with 2550 relay fee", size: 782, minimum_relay_transaction_fee: 2550, want: 1994 },
        ];

        for test in tests.iter() {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                let got = mempool.minimum_required_transaction_relay_fee(test.size);
                if got != test.want {
                    println!("test_calc_min_required_tx_relay_fee test '{}' failed: got {}, want {}", test.name, got, test.want);
                }
                assert_eq!(test.want, got);
            }
        }
    }

    #[test]
    fn test_is_transaction_output_dust() {
        let script_public_key = vec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e,
        ];
        let invalid_script_public_key = vec![0x01];

        struct Test {
            name: &'static str,
            cell_out: CellOutput,
            minimum_relay_transaction_fee: u64,
            is_dust: bool,
        }

        // Helper to create CellOutput from capacity and script_public_key
        let make_cell_out =
            |capacity: u64, script: Vec<u8>| -> CellOutput { CellOutput { capacity, lock: lock_script(script), type_: None } };

        let tests = vec![
            // Any value is allowed with a zero relay fee.
            Test {
                name: "zero value with zero relay fee",
                cell_out: make_cell_out(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 0,
                is_dust: false,
            },
            // Zero value is dust with any relay fee"
            Test {
                name: "zero value with very small tx fee",
                cell_out: make_cell_out(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 1,
                is_dust: true,
            },
            Test {
                name: "36 byte public key script with value 605",
                cell_out: make_cell_out(605, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: true,
            },
            Test {
                name: "36 byte public key script with value 606",
                cell_out: make_cell_out(606, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: true,
            },
            // Maximum allowed value is never dust.
            Test {
                name: "max sau amount is never dust",
                cell_out: make_cell_out(MAX_SAU, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Maximum uint64 value causes NO overflow.
            // Rust rewrite: caution, this differs from the golang version
            Test {
                name: "maximum uint64 value",
                cell_out: make_cell_out(u64::MAX, script_public_key),
                minimum_relay_transaction_fee: u64::MAX,
                is_dust: false,
            },
            // Opaque one-byte scripts are not currently treated as unspendable by the Cell path.
            Test {
                name: "opaque one-byte script remains non-dust at zero relay fee",
                cell_out: make_cell_out(5000, invalid_script_public_key),
                minimum_relay_transaction_fee: 0,
                is_dust: false,
            },
        ];
        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                println!("test_is_transaction_output_dust test '{}' ", test.name);
                let res = mempool.is_transaction_output_dust(&test.cell_out);
                if res != test.is_dust {
                    println!("test_is_transaction_output_dust test '{}' failed: got {}, want {}", test.name, res, test.is_dust);
                }
                assert_eq!(test.is_dust, res);
            }
        }
    }

    #[test]
    fn test_check_transaction_standard_in_isolation() {
        // Create some dummy, but otherwise standard, data for transactions.
        let dummy_prev_out = TransactionOutpoint::new(spora_hashes::Hash::from_u64_word(1).as_bytes(), 1);
        let dummy_sig_script = vec![0u8; 65];
        let dummy_tx_input = CellInput::new(dummy_prev_out, MAX_TX_IN_SEQUENCE_NUM);
        let addr_hash = vec![1u8; 32];

        let addr = Address::new(Prefix::Testnet, Version::PubKey, &addr_hash).expect("Valid test address");
        let dummy_lock_script = pay_to_address_lock_script(&addr);
        let dummy_tx_out = CellOutput { capacity: SAU_PER_SPORA, lock: dummy_lock_script.clone(), type_: None };

        struct Test {
            name: &'static str,
            mtx: MutableTransaction,
            is_standard: bool,
        }

        fn new_mtx(tx: CellTx, mass: u64) -> MutableTransaction {
            let mut mtx = MutableTransaction::from_cell_tx(tx);
            mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(mass, mass));
            mtx.calculated_contextual_masses = Some(ContextualMasses::new(mass));
            mtx
        }

        let tests = vec![
            Test {
                name: "Typical pay-to-pubkey transaction",
                mtx: new_mtx(
                    CellTx::new(
                        vec![dummy_tx_input.clone()],
                        vec![],
                        vec![dummy_tx_out.clone()],
                        vec![vec![]],
                        vec![dummy_sig_script.clone()],
                    )
                    .expect("test helper must construct a valid CellTx"),
                    1000,
                ),
                is_standard: true,
            },
            Test {
                name: "Transaction version too high",
                mtx: new_mtx(
                    {
                        let mut tx = CellTx::new(
                            vec![dummy_tx_input.clone()],
                            vec![],
                            vec![dummy_tx_out.clone()],
                            vec![vec![]],
                            vec![dummy_sig_script.clone()],
                        )
                        .expect("test helper must construct a valid CellTx");
                        tx.version = CELL_TX_VERSION + 1;
                        tx
                    },
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Transaction size is too large",
                mtx: new_mtx(
                    CellTx::new(
                        vec![dummy_tx_input.clone()],
                        vec![],
                        vec![CellOutput {
                            capacity: 0,
                            lock: lock_script(vec![0u8; MAXIMUM_STANDARD_TRANSACTION_MASS as usize + 1]),
                            type_: None,
                        }],
                        vec![vec![]],
                        vec![dummy_sig_script.clone()],
                    )
                    .expect("test helper must construct a valid CellTx"),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Signature script size is too large",
                mtx: new_mtx(
                    {
                        let mut tx = CellTx::new(
                            vec![CellInput::new(dummy_prev_out, MAX_TX_IN_SEQUENCE_NUM)],
                            vec![],
                            vec![dummy_tx_out.clone()],
                            vec![vec![]],
                            vec![vec![0u8; MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE as usize + 1]],
                        )
                        .expect("test helper must construct a valid CellTx");
                        tx.version = CELL_TX_VERSION + 1;
                        tx
                    },
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Valid opaque script is currently accepted in isolation",
                mtx: new_mtx(
                    CellTx::new(
                        vec![dummy_tx_input.clone()],
                        vec![],
                        vec![CellOutput { capacity: SAU_PER_SPORA, lock: lock_script(vec![OP_TRUE]), type_: None }],
                        vec![vec![]],
                        vec![dummy_sig_script.clone()],
                    )
                    .expect("test helper must construct a valid CellTx"),
                    1000,
                ),
                is_standard: true,
            },
            Test {
                name: "Dust output",
                mtx: new_mtx(
                    CellTx::new(
                        vec![dummy_tx_input.clone()],
                        vec![],
                        vec![CellOutput { capacity: 0, lock: dummy_lock_script.clone(), type_: None }],
                        vec![vec![]],
                        vec![dummy_sig_script.clone()],
                    )
                    .expect("test helper must construct a valid CellTx"),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Lock script bytes starting with op-return in args are currently accepted",
                mtx: new_mtx(
                    CellTx::new(
                        vec![dummy_tx_input],
                        vec![],
                        vec![CellOutput { capacity: SAU_PER_SPORA, lock: lock_script(vec![OP_RETURN]), type_: None }],
                        vec![vec![]],
                        vec![dummy_sig_script],
                    )
                    .expect("test helper must construct a valid CellTx"),
                    1000,
                ),
                is_standard: true,
            },
        ];

        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                // Ensure standard-ness is as expected.
                println!("test_check_transaction_standard_in_isolation test '{}' ", test.name);
                let res = mempool.check_transaction_standard_in_isolation(&test.mtx);
                if res.is_ok() && test.is_standard {
                    // Test passes since function returned standard for a
                    // transaction which is intended to be standard.
                    continue;
                }
                if res.is_ok() && !test.is_standard {
                    println!("test_check_transaction_standard_in_isolation ({}): standard when it should not be", test.name);
                }
                if res.is_err() && test.is_standard {
                    println!(
                        "test_check_transaction_standard_in_isolation ({}): nonstandard when it should not be: {:?}",
                        test.name, res
                    );
                }
                assert_eq!(res.is_ok(), test.is_standard, "ensuring transaction standard-ness is as expected");
            }
        }
    }

    #[test]
    fn test_check_transaction_standard_in_context_accepts_cell_placeholder_inputs() {
        let params: Params = NetworkType::Mainnet.into();
        let config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), counters);

        let previous_outpoint = TransactionOutpoint::new(spora_hashes::Hash::from_u64_word(7).as_bytes(), 0);
        let output = CellOutput {
            capacity: 900,
            lock: Script::new(
                [0x20, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                0,
                vec![1, 0xac],
            ),
            type_: None,
        };
        let tx = CellTx::new(
            vec![CellInput::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
            vec![],
            vec![output],
            vec![vec![]],
            vec![vec![0u8; 64]],
        )
        .expect("test helper must construct a valid CellTx");
        let mut mtx = MutableTransaction::from_cell_tx(tx);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1000, 1000));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(1000));
        mtx.calculated_fee = Some(DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        mtx.entries[0] = Some(CellMeta::from_cell_metadata(SAU_PER_SPORA, 0, [0x44; 32], None, [0; 32], 100, false));

        assert!(mempool.check_transaction_standard_in_context(&mtx).is_ok());
    }

    #[test]
    fn test_check_transaction_standard_in_context_accepts_metadata_only_inputs() {
        let params: Params = NetworkType::Mainnet.into();
        let config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), counters);

        let previous_outpoint = TransactionOutpoint::new(spora_hashes::Hash::from_u64_word(9).as_bytes(), 0);
        let output = CellOutput {
            capacity: 900,
            lock: Script::new(
                [0x20, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                0,
                vec![1, 0xac],
            ),
            type_: None,
        };
        let tx = CellTx::new(
            vec![CellInput::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
            vec![],
            vec![output],
            vec![vec![]],
            vec![vec![0u8; 64]],
        )
        .expect("test helper must construct a valid CellTx");
        let mut mtx = MutableTransaction::from_cell_tx(tx);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1000, 1000));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(1000));
        mtx.calculated_fee = Some(DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        mtx.resolved_cell_metadata[0] = Some(CellMetadata {
            out_point: previous_outpoint,
            capacity: SAU_PER_SPORA,
            data_bytes: 0,
            lock_hash: [0x44; 32],
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 100,
            is_cellbase: false,
            block_hash: spora_hashes::Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        });

        assert!(mempool.check_transaction_standard_in_context(&mtx).is_ok());
    }

    #[test]
    fn test_check_transaction_standard_in_context_rejects_fee_below_selection_mass_floor() {
        let params: Params = NetworkType::Mainnet.into();
        let config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), counters);

        let previous_outpoint = TransactionOutpoint::new(spora_hashes::Hash::from_u64_word(11).as_bytes(), 0);
        let output = CellOutput {
            capacity: 900,
            lock: Script::new(
                [0x20, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                0,
                vec![1, 0xac],
            ),
            type_: None,
        };
        let tx = CellTx::new(
            vec![CellInput::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
            vec![],
            vec![output],
            vec![vec![]],
            vec![vec![0u8; 64]],
        )
        .expect("test helper must construct a valid CellTx");
        let mut mtx = MutableTransaction::from_cell_tx(tx);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1_000, 1_000));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(5_000));
        mtx.calculated_fee = Some(DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        mtx.resolved_cell_metadata[0] = Some(CellMetadata {
            out_point: previous_outpoint,
            capacity: SAU_PER_SPORA,
            data_bytes: 0,
            lock_hash: [0x44; 32],
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 100,
            is_cellbase: false,
            block_hash: spora_hashes::Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        });

        let err = mempool
            .check_transaction_standard_in_context(&mtx)
            .expect_err("fee should be evaluated against selection mass, not compute mass");
        assert!(matches!(
            err,
            NonStandardError::RejectInsufficientFee(_, fee, minimum_fee)
                if fee == DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE && minimum_fee == 5_000
        ));
    }

    #[test]
    fn test_check_transaction_standard_in_context_accepts_fee_matching_selection_mass_floor() {
        let params: Params = NetworkType::Mainnet.into();
        let config = Config::build_default(params.target_time_per_block(), false, params.max_block_mass);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), counters);

        let previous_outpoint = TransactionOutpoint::new(spora_hashes::Hash::from_u64_word(13).as_bytes(), 0);
        let output = CellOutput {
            capacity: 900,
            lock: Script::new(
                [0x20, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                0,
                vec![1, 0xac],
            ),
            type_: None,
        };
        let tx = CellTx::new(
            vec![CellInput::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
            vec![],
            vec![output],
            vec![vec![]],
            vec![vec![0u8; 64]],
        )
        .expect("test helper must construct a valid CellTx");
        let mut mtx = MutableTransaction::from_cell_tx(tx);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1_000, 1_000));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(5_000));
        mtx.calculated_fee = Some(5_000);
        mtx.resolved_cell_metadata[0] = Some(CellMetadata {
            out_point: previous_outpoint,
            capacity: SAU_PER_SPORA,
            data_bytes: 0,
            lock_hash: [0x44; 32],
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 100,
            is_cellbase: false,
            block_hash: spora_hashes::Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        });

        assert!(mempool.check_transaction_standard_in_context(&mtx).is_ok());
    }
}
