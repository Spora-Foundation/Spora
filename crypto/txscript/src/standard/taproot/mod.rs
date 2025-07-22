pub mod witness;

#[cfg(test)]
mod tests {
    use crate::{
        caches::Cache,
        opcodes::codes::{OpData32, OpTrue},
        TxScriptEngine, Witness,
    };
    use bitcoin::key::{TapTweak, TweakedPublicKey};
    use secp256k1::{Keypair, Message, Secp256k1};
    use smallvec::SmallVec;
    use std::str::FromStr;
    use tondi_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        subnets::SubnetworkId,
        tx::{
            taproot::sighash::{Prevouts, SighashCache, TapSighashType},
            PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        },
    };
    use tondi_utils::hex::FromHex;

    #[test]
    fn test_taproot_key_spend_scenario() {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            &Vec::from_hex("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160").unwrap(),
        )
        .unwrap();
        let tweaked = keypair.tap_tweak(&secp, None);
        let tweaked_pub_key = TweakedPublicKey::from_keypair(tweaked);
        let script_pub_key = SmallVec::from_iter([OpTrue, OpData32].into_iter().chain(tweaked_pub_key.serialize()));

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();

        let mut tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let utxos = vec![TransactionOutput::new(100, ScriptPublicKey::new(0, script_pub_key.clone()))];
        let prevouts = Prevouts::All(&utxos);
        let input_index = 0;
        let sighash_type = TapSighashType::Default;
        let mut sighasher = SighashCache::new(&tx);
        let sighash =
            sighasher.taproot_key_spend_signature_hash(input_index, &prevouts, sighash_type).expect("failed to construct sighash");

        assert_eq!(format!("{sighash}"), "d8452beb5ba4bddd8509763b6c128f04b9cfaffa5e385c5a1011d98d60bdcb0c");

        let msg = Message::from(sighash);
        let signature = secp.sign_schnorr(&msg, tweaked.as_keypair());
        let witness = Witness::p2tr_key_spend(signature, sighash_type.into());
        tx.inputs[input_index].signature_script = (&witness).try_into().unwrap();

        let entry = UtxoEntry {
            amount: 100,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let reused_values = SigHashReusedValuesUnsync::new();
        let cache = Cache::new(10_000);
        let populated_tx = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        let mut engine = TxScriptEngine::from_transaction_input(
            &populated_tx,
            &tx.inputs[input_index],
            input_index,
            &entry,
            &reused_values,
            &cache,
            false,
            false,
        );
        assert!(engine.execute().is_ok());
    }
}
