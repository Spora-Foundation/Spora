use super::HasherExtensions;
use crate::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput};
use tondi_hashes::{Hash, Hasher};

/// A bitmask defining which transaction fields we
/// want to encode and which to ignore.
type TxEncodingFlags = u8;

pub const TX_ENCODING_FULL: TxEncodingFlags = 0;
pub const TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT: TxEncodingFlags = 1;

/// Returns the transaction hash. Note that this is different than the transaction ID.
pub fn hash(tx: &Transaction, include_mass_field: bool) -> Hash {
    let mut hasher = tondi_hashes::TransactionHash::new();
    write_transaction(&mut hasher, tx, TX_ENCODING_FULL, include_mass_field);
    hasher.finalize()
}

/// Not intended for direct use by clients. Instead use `tx.id()`
pub(crate) fn id(tx: &Transaction) -> TransactionId {
    // Encode the transaction, replace signature script with an empty array, skip
    // sigop counts and mass and hash the result.

    let encoding_flags = if tx.is_coinbase() { TX_ENCODING_FULL } else { TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT };
    let mut hasher = tondi_hashes::TransactionID::new();
    write_transaction(&mut hasher, tx, encoding_flags, false);
    hasher.finalize()
}

/// Write the transaction into the provided hasher according to the encoding flags
fn write_transaction<T: Hasher>(hasher: &mut T, tx: &Transaction, encoding_flags: TxEncodingFlags, include_mass_field: bool) {
    hasher.update(tx.version.to_le_bytes()).write_len(tx.inputs.len());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input, encoding_flags);
    }

    hasher.write_len(tx.outputs.len());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output);
    }

    hasher.update(tx.lock_time.to_le_bytes()).update(&tx.subnetwork_id).update(tx.gas.to_le_bytes()).write_var_bytes(&tx.payload);

    /*
       Design principles (mostly related to the new mass commitment field; see KIP-0009):
           1. The new mass field should not modify tx::id (since it is essentially a commitment by the miner re block space usage
              so there is no need to modify the id definition which will require wide-spread changes in ecosystem software).
           2. Coinbase tx hash and id should ideally remain equal

       Solution:
           1. Hash the mass field only for tx::hash
           2. Hash the mass field only if mass > 0
           3. Require in consensus that coinbase mass == 0

       This way we have:
           - Unique commitment for tx::hash per any possible mass value (with only zero being a no-op)
           - tx::id remains unmodified
           - Coinbase tx hash and id remain the same and equal
    */

    // TODO (post HF):
    //      1. Avoid passing a boolean
    //      2. Use TxEncodingFlags to avoid including the mass for tx ID
    if include_mass_field {
        let mass = tx.mass();
        if mass > 0 {
            hasher.update(mass.to_le_bytes());
        }
    }
}

#[inline(always)]
fn write_input<T: Hasher>(hasher: &mut T, input: &TransactionInput, encoding_flags: TxEncodingFlags) {
    write_outpoint(hasher, &input.previous_outpoint);
    if encoding_flags & TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT != TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT {
        hasher.write_var_bytes(input.signature_script.as_slice()).update([input.sig_op_count]);
    } else {
        hasher.write_var_bytes(&[]);
    }
    hasher.update(input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint<T: Hasher>(hasher: &mut T, outpoint: &TransactionOutpoint) {
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output<T: Hasher>(hasher: &mut T, output: &TransactionOutput) {
    hasher
        .update(output.value.to_le_bytes())
        .update(output.script_public_key.version().to_le_bytes())
        .write_var_bytes(output.script_public_key.script());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::{self, SubnetworkId},
        tx::{scriptvec, ScriptPublicKey},
    };
    use std::str::FromStr;

    #[test]
    fn test_transaction_hashing() {
        struct Test {
            tx: Transaction,
            expected_id: &'static str,
            expected_hash: &'static str,
        }

        let mut tests = vec![
            // Test #1
            Test {
                tx: Transaction::new(0, Vec::new(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
                expected_id: "9ad6a3c5f9a91ef4e16cf1dda6e909ea270b9576e34fd8c40b9dbc2d44be6eab",
                expected_hash: "5e74f83dbc70f84bede6d3ee101da4fc647d61af7e692e543af77e94e5a3e674",
            },
        ];

        let inputs = vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(0), 2), vec![1, 2], 7, 5)];

        // Test #2
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "b91f6377d9a0f84ce98886db3eb6b003bbade598cf99d7b4875798cfdb96e638",
            expected_hash: "760cb11a9daa8d15fd58b0647452bf772982c892b2bc97d101753ab3f438039f",
        });

        let outputs = vec![TransactionOutput::new(1564, ScriptPublicKey::new(7, scriptvec![1, 2, 3, 4, 5]))];

        // Test #3
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), outputs.clone(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "97876e81cc43c1b7e3097ba606a953575eae1a83f375c1e5fe7b2b7615a7b3f5",
            expected_hash: "54d9fe8b483c77761322ea703539c856b813d03fee265983935d9cac0f3b8ba0",
        });

        // Test #4
        tests.push(Test {
            tx: Transaction::new(2, inputs, outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: "c3a195b936648a951c5d8437055eee93d6517278692460eadd68840989efcd1b",
            expected_hash: "de99f46355d7ad26368b83e856d0cb741b7c1d4ca3a4d33988fc83ff2db9796b",
        });

        let inputs = vec![TransactionInput::new(
            TransactionOutpoint::new(Hash::from_str("59b3d6dc6cdc660c389c3fdb5704c48c598d279cdf1bab54182db586a4c95dd5").unwrap(), 2),
            vec![1, 2],
            7,
            5,
        )];

        // Test #5
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: "6e7785715267573c7607444c9b8394a017b04a78e752c1b63e128c1f3cad7d58",
            expected_hash: "c50894ec6563466e0b5f5f5830c3bcf30a2152235ba0bef56d434aafc3a920d7",
        });

        // Test #6
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_COINBASE, 3, Vec::new()),
            expected_id: "ddd5d48eb159ad6f761b3edcaa60b05a9325f6688fde8303e28915e0577b1fdc",
            expected_hash: "9dbb3e95c4d07b492bd385605fcf5f8ccb93e4fb1c1738cd9ffc9d14ea02288f",
        });

        // Test #7
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, Vec::new()),
            expected_id: "4b6da5bd872acfd125b61824bbbcd0692b172c2fbb819a77c5b79cc60232f301",
            expected_hash: "08b46a1d8966902767384329dabbcb8f2ad4cf21977bb7c212ff16b1d34f89d5",
        });

        // Test #8
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, vec![1, 2, 3]),
            expected_id: "0cc4328dda6da37d87851d016d37e96784033db35f3dae488207fe6b8519b87e",
            expected_hash: "8f88bf6b299011c9fb0f8227c6163c4fffa1e87633edd54df013c65fe145e312",
        });

        for (i, test) in tests.iter().enumerate() {
            assert_eq!(test.tx.id(), Hash::from_str(test.expected_id).unwrap(), "transaction id failed for test {}", i + 1);
            assert_eq!(
                hash(&test.tx, false),
                Hash::from_str(test.expected_hash).unwrap(),
                "transaction hash failed for test {}",
                i + 1
            );
        }

        // Avoid compiler warnings on the last clone
        drop(inputs);
        drop(outputs);
    }
}
