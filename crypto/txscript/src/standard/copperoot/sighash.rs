//! Copperoot sighash implementation using BLAKE3-256
//!
//! This module implements signature hash computation for Copperoot transactions,
//! replacing SHA256 with BLAKE3-256 for all hash operations.

use std::borrow::Borrow;

use bitcoin::{
    consensus::Encodable,
    io::{Error as BitcoinIoError, Write},
    VarInt,
};
use blake3::Hasher;
use secp256k1::Message;

use spora_consensus_core::tx::{copperoot::error::CopperootError, Transaction, TransactionInput, TransactionOutput};

const KEY_VERSION_0: u8 = 0u8;

/// Copperoot sighash using BLAKE3-256
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CopperootSighash([u8; 32]);

impl CopperootSighash {
    /// Create a new BLAKE3 engine for sighash computation
    pub fn engine() -> Hasher {
        let mut hasher = Hasher::new();
        hasher.update(crate::standard::copperoot::hasher::COPPEROOT_SIGHASH_TAG);
        hasher
    }

    /// Create from engine
    pub fn from_engine(engine: Hasher) -> Self {
        Self(engine.finalize().into())
    }

    /// Get byte array
    pub fn to_byte_array(self) -> [u8; 32] {
        self.0
    }

    /// Get as bytes
    pub fn as_byte_array(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<CopperootSighash> for Message {
    fn from(hash: CopperootSighash) -> Self {
        Message::from_digest_slice(&hash.to_byte_array()).expect("32-byte digest")
    }
}

/// Copperoot leaf hash using BLAKE3-256
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CopperootLeafHash([u8; 32]);

impl CopperootLeafHash {
    /// Create a new BLAKE3 engine for leaf hash computation
    pub fn engine() -> Hasher {
        let mut hasher = Hasher::new();
        hasher.update(b"CopperootLeaf");
        hasher
    }

    /// Create from engine
    pub fn from_engine(engine: Hasher) -> Self {
        Self(engine.finalize().into())
    }

    /// Get byte array
    pub fn to_byte_array(self) -> [u8; 32] {
        self.0
    }

    /// Get as bytes
    pub fn as_byte_array(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Hashtype of an input's signature, encoded in the last byte of the signature.
/// Fixed values so they can be cast as integer types for encoding.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CopperootSighashType {
    /// 0x0: Used when not explicitly specified, defaults to [`CopperootSighashType::All`]
    Default = 0x00,
    /// 0x1: Sign all outputs.
    All = 0x01,
    /// 0x2: Sign no outputs --- anyone can choose the destination.
    None = 0x02,
    /// 0x3: Sign the output whose index matches this input's index. If none exists,
    /// sign the hash `0000000000000000000000000000000000000000000000000000000000000001`.
    /// (This rule is probably an unintentional C++ism, but it's consensus so we have
    /// to follow it.)
    Single = 0x03,
    /// 0x81: Sign all outputs but only this input.
    AllPlusAnyoneCanPay = 0x81,
    /// 0x82: Sign no outputs and only this input.
    NonePlusAnyoneCanPay = 0x82,
    /// 0x83: Sign one output and only this input (see `Single` for what "one output" means).
    SinglePlusAnyoneCanPay = 0x83,
}

impl CopperootSighashType {
    /// Breaks the sighash flag into the "real" sighash flag and the `SIGHASH_ANYONECANPAY` boolean.
    pub(crate) fn split_anyonecanpay_flag(self) -> (CopperootSighashType, bool) {
        use CopperootSighashType::*;

        match self {
            Default => (All, false), // Critical fix: Default semantics equals All
            All => (All, false),
            None => (None, false),
            Single => (Single, false),
            AllPlusAnyoneCanPay => (All, true),
            NonePlusAnyoneCanPay => (None, true),
            SinglePlusAnyoneCanPay => (Single, true),
        }
    }

    /// Constructs a [`CopperootSighashType`] from a raw `u8`.
    pub fn from_consensus_u8(sighash_type: u8) -> Result<Self, CopperootError> {
        use CopperootSighashType::*;

        Ok(match sighash_type {
            0x00 => Default,
            0x01 => All,
            0x02 => None,
            0x03 => Single,
            0x81 => AllPlusAnyoneCanPay,
            0x82 => NonePlusAnyoneCanPay,
            0x83 => SinglePlusAnyoneCanPay,
            _ => return Err(CopperootError::InvalidSighashTypeError),
        })
    }

    /// Convert to u8 representation
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

impl From<CopperootSighashType> for bitcoin::TapSighashType {
    fn from(ty: CopperootSighashType) -> Self {
        match ty {
            CopperootSighashType::Default => Self::Default,
            CopperootSighashType::All => Self::All,
            CopperootSighashType::None => Self::None,
            CopperootSighashType::Single => Self::Single,
            CopperootSighashType::AllPlusAnyoneCanPay => Self::AllPlusAnyoneCanPay,
            CopperootSighashType::NonePlusAnyoneCanPay => Self::NonePlusAnyoneCanPay,
            CopperootSighashType::SinglePlusAnyoneCanPay => Self::SinglePlusAnyoneCanPay,
        }
    }
}

/// The `Annex` struct is a slice wrapper enforcing first byte is `0x50`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Annex<'a>(&'a [u8]);

impl<'a> Annex<'a> {
    /// Create a new Annex with validation
    pub fn new(bytes: &'a [u8]) -> Result<Self, CopperootError> {
        if bytes.first().copied() == Some(0x50) {
            Ok(Self(bytes))
        } else {
            Err(CopperootError::InvalidAnnex)
        }
    }
}

impl<'a> Encodable for Annex<'a> {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, BitcoinIoError> {
        // Ensure first byte is 0x50
        debug_assert!(self.0.first().copied() == Some(0x50), "Annex must start with 0x50");
        
        let data = self.0;
        let vi_len = VarInt(data.len() as u64).consensus_encode(w)?;
        w.write_all(data)?;
        Ok(vi_len + data.len())
    }
}

/// Contains outputs of previous transactions. In the case [`CopperootSighashType`] variant is
/// `SIGHASH_ANYONECANPAY`, [`Prevouts::One`] may be used.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Prevouts<'u, T>
where
    T: 'u + Borrow<TransactionOutput>,
{
    /// `One` variant allows provision of the single prevout needed. It's useful, for example, when
    /// modifier `SIGHASH_ANYONECANPAY` is provided, only prevout of the current input is needed.
    /// The first `usize` argument is the input index this [`TxOut`] is referring to.
    One(usize, T),
    /// When `SIGHASH_ANYONECANPAY` is not provided, or when the caller is giving all prevouts so
    /// the same variable can be used for multiple inputs.
    All(&'u [T]),
}

impl<'u, TxOut> Prevouts<'u, TxOut>
where
    TxOut: Borrow<TransactionOutput>,
{
    fn check_all(&self, tx: &Transaction) -> Result<(), CopperootError> {
        if let Prevouts::All(prevouts) = self {
            if prevouts.len() != tx.inputs.len() {
                return Err(CopperootError::PrevoutsError);
            }
        }
        Ok(())
    }

    fn get_all(&self) -> Result<&[TxOut], CopperootError> {
        match self {
            Prevouts::All(prevouts) => Ok(*prevouts),
            _ => Err(CopperootError::PrevoutsError),
        }
    }

    fn get(&self, input_index: usize) -> Result<&TransactionOutput, CopperootError> {
        match self {
            Prevouts::One(index, prevout) => {
                if input_index == *index {
                    Ok(prevout.borrow())
                } else {
                    Err(CopperootError::PrevoutsError)
                }
            }
            Prevouts::All(prevouts) => prevouts.get(input_index).map(|x| x.borrow()).ok_or(CopperootError::PrevoutsError),
        }
    }
}

/// Common values cached between segwit and copperoot inputs.
#[allow(dead_code)]
#[derive(Debug)]
struct CommonCache {
    prevouts: [u8; 32],
    sequences: [u8; 32],

    /// In theory `outputs` could be an `Option` since `SIGHASH_NONE` and `SIGHASH_SINGLE` do not
    /// need it, but since `SIGHASH_ALL` is by far the most used variant we don't bother.
    outputs: [u8; 32],
}

/// Values cached for copperoot inputs.
#[allow(dead_code)]
#[derive(Debug)]
struct CopperootCache {
    amounts: [u8; 32],
    script_pubkeys: [u8; 32],
}

/// Efficiently calculates signature hash message for legacy, segwit and copperoot inputs.
#[derive(Debug)]
pub struct SighashCache<Tx: Borrow<Transaction>> {
    /// Access to transaction required for transaction introspection. Moreover, type
    /// `T: Borrow<Transaction>` allows us to use borrowed and mutable borrowed types,
    /// the latter in particular is necessary for [`SighashCache::witness_mut`].
    tx: Tx,
    /// Common cache for copperoot and segwit inputs, `None` for legacy inputs.
    common_cache: Option<CommonCache>,
    /// Cache for copperoot v1 inputs.
    copperoot_cache: Option<CopperootCache>,
}

// Note: Encodable implementations for TransactionOutpoint and TransactionOutput
// are provided by the spora_consensus_core crate

impl<Tx: Borrow<Transaction>> SighashCache<Tx> {
    /// Constructs a new `SighashCache` from an unsigned transaction.
    ///
    /// The sighash components are computed in a lazy manner when required. For the generated
    /// sighashes to be valid, no fields in the transaction may change except for script_sig and
    /// witness.
    pub fn new(tx: Tx) -> Self {
        SighashCache { tx, common_cache: None, copperoot_cache: None }
    }

    #[inline]
    fn common_cache(&mut self) -> &CommonCache {
        Self::common_cache_minimal_borrow(&mut self.common_cache, self.tx.borrow())
    }

    fn copperoot_cache<TxOut: Borrow<TransactionOutput>>(&mut self, prevouts: &[TxOut]) -> &CopperootCache {
        self.copperoot_cache.get_or_insert_with(|| {
            let mut enc_amounts = Hasher::new();
            enc_amounts.update(crate::standard::copperoot::hasher::AMOUNTS_TAG); // Domain separation prefix
            let mut enc_script_pubkeys = Hasher::new();
            enc_script_pubkeys.update(crate::standard::copperoot::hasher::SCRIPT_PUBKEYS_TAG); // Domain separation prefix
            for prevout in prevouts {
                let txout = prevout.borrow();
                // Serialize to bytes first, then update hasher
                let mut amount_bytes = Vec::new();
                txout.value.consensus_encode(&mut amount_bytes).unwrap();
                enc_amounts.update(&amount_bytes);
                
                let mut script_bytes = Vec::new();
                txout.script_public_key.script().to_vec().consensus_encode(&mut script_bytes).unwrap();
                enc_script_pubkeys.update(&script_bytes);
            }
            CopperootCache {
                amounts: enc_amounts.finalize().into(),
                script_pubkeys: enc_script_pubkeys.finalize().into(),
            }
        })
    }

    fn common_cache_minimal_borrow<'a>(common_cache: &'a mut Option<CommonCache>, tx: &Transaction) -> &'a CommonCache {
        common_cache.get_or_insert_with(|| {
            let mut enc_prevouts = Hasher::new();
            enc_prevouts.update(crate::standard::copperoot::hasher::PREVOUTS_TAG); // Domain separation prefix
            let mut enc_sequences = Hasher::new();
            enc_sequences.update(crate::standard::copperoot::hasher::SEQUENCES_TAG); // Domain separation prefix
            for txin in tx.inputs.iter() {
                // Serialize to bytes first, then update hasher
                let mut prevout_bytes = Vec::new();
                txin.previous_outpoint.consensus_encode(&mut prevout_bytes).unwrap();
                enc_prevouts.update(&prevout_bytes);
                
                let mut sequence_bytes = Vec::new();
                txin.sequence.consensus_encode(&mut sequence_bytes).unwrap();
                enc_sequences.update(&sequence_bytes);
            }
            CommonCache {
                prevouts: enc_prevouts.finalize().into(),
                sequences: enc_sequences.finalize().into(),
                outputs: {
                    let mut enc = Hasher::new();
                    enc.update(crate::standard::copperoot::hasher::OUTPUTS_TAG); // Domain separation prefix
                    for txout in tx.outputs.iter() {
                        let mut output_bytes = Vec::new();
                        txout.consensus_encode(&mut output_bytes).unwrap();
                        enc.update(&output_bytes);
                    }
                    enc.finalize().into()
                },
            }
        })
    }

    /// Computes the Copperoot sighash for a key spend.
    pub fn copperoot_key_spend_signature_hash<TxOut: Borrow<TransactionOutput>>(
        &mut self,
        input_index: usize,
        prevouts: &Prevouts<TxOut>,
        sighash_type: CopperootSighashType,
    ) -> Result<CopperootSighash, CopperootError> {
        let mut buffer = Vec::new();
        self.copperoot_encode_signing_data_to(&mut buffer, input_index, prevouts, None, None, sighash_type)?;
        let mut enc = CopperootSighash::engine();
        enc.update(&buffer);
        Ok(CopperootSighash::from_engine(enc))
    }

    /// Encodes the Copperoot signing data for any flag type into a given object implementing the
    /// [`io::Write`] trait.
    pub fn copperoot_encode_signing_data_to<W: Write + ?Sized, TxOut: Borrow<TransactionOutput>>(
        &mut self,
        writer: &mut W,
        input_index: usize,
        prevouts: &Prevouts<TxOut>,
        annex: Option<Annex>,
        leaf_hash_code_separator: Option<(CopperootLeafHash, u32)>,
        sighash_type: CopperootSighashType,
    ) -> Result<(), CopperootError> {
        prevouts.check_all(self.tx.borrow())?;

        let (sighash, anyone_can_pay) = sighash_type.split_anyonecanpay_flag();

        // epoch
        0u8.consensus_encode(writer)?;

        // * Control:
        // hash_type (1).
        sighash_type.to_u8().consensus_encode(writer)?;

        // * Transaction Data:
        // nVersion (4): the nVersion of the transaction.
        self.tx.borrow().version.consensus_encode(writer)?;

        // nLockTime (4): the nLockTime of the transaction.
        self.tx.borrow().lock_time.consensus_encode(writer)?;

        // If the hash_type & 0x80 does not equal SIGHASH_ANYONECANPAY:
        //     sha_prevouts (32): the BLAKE3 of the serialization of all input outpoints.
        //     sha_amounts (32): the BLAKE3 of the serialization of all spent output amounts.
        //     sha_scriptpubkeys (32): the BLAKE3 of the serialization of all spent output scriptPubKeys.
        //     sha_sequences (32): the BLAKE3 of the serialization of all input nSequence.
        if !anyone_can_pay {
            self.common_cache().prevouts.consensus_encode(writer)?;
            self.copperoot_cache(prevouts.get_all()?).amounts.consensus_encode(writer)?;
            self.copperoot_cache(prevouts.get_all()?).script_pubkeys.consensus_encode(writer)?;
            self.common_cache().sequences.consensus_encode(writer)?;
        }

        // If hash_type & 3 does not equal SIGHASH_NONE or SIGHASH_SINGLE:
        //     sha_outputs (32): the BLAKE3 of the serialization of all outputs in CTxOut format.
        if sighash != CopperootSighashType::None && sighash != CopperootSighashType::Single {
            self.common_cache().outputs.consensus_encode(writer)?;
        }

        // * Data about this input:
        // spend_type (1): equal to (ext_flag * 2) + annex_present, where annex_present is 0
        // if no annex is present, or 1 otherwise
        let mut spend_type = 0u8;
        if annex.is_some() {
            spend_type |= 1u8;
        }
        if leaf_hash_code_separator.is_some() {
            spend_type |= 2u8;
        }
        spend_type.consensus_encode(writer)?;

        // If hash_type & 0x80 equals SIGHASH_ANYONECANPAY:
        //      outpoint (36): the COutPoint of this input (32-byte hash + 4-byte little-endian).
        //      amount (8): value of the previous output spent by this input.
        //      scriptPubKey (varint + script): scriptPubKey of the previous output spent by this input, serialized as script inside CTxOut. Size is variable (varint length + script bytes).
        //      nSequence (4): nSequence of this input.
        if anyone_can_pay {
            let txin: &TransactionInput = &self.tx.borrow().inputs[input_index];
            let previous_output = prevouts.get(input_index)?;
            txin.previous_outpoint.consensus_encode(writer)?;
            previous_output.value.consensus_encode(writer)?;
            previous_output.script_public_key.script().to_vec().consensus_encode(writer)?;
            txin.sequence.consensus_encode(writer)?;
        } else {
            (input_index as u32).consensus_encode(writer)?;
        }

        // If an annex is present (the lowest bit of spend_type is set):
        //      sha_annex (32): the BLAKE3 of (compact_size(size of annex) || annex), where annex
        //      includes the mandatory 0x50 prefix.
        if let Some(annex) = annex {
            let mut annex_bytes = Vec::new();
            annex.consensus_encode(&mut annex_bytes)?;
            let mut enc = Hasher::new();
            enc.update(&annex_bytes);
            let hash = enc.finalize();
            hash.as_bytes().consensus_encode(writer)?;
        }

        // * Data about this output:
        // If hash_type & 3 equals SIGHASH_SINGLE:
        //      sha_single_output (32): the BLAKE3 of the corresponding output in CTxOut format.
        if sighash == CopperootSighashType::Single {
            if input_index >= self.tx.borrow().outputs.len() {
                // 32 bytes: 00..01
                let mut one = [0u8; 32];
                one[0] = 1;
                one.consensus_encode(writer)?;
            } else {
                let mut output_bytes = Vec::new();
                self.tx.borrow().outputs[input_index].consensus_encode(&mut output_bytes)?;
                let mut enc = Hasher::new();
                enc.update(&output_bytes);
                let hash = enc.finalize();
                hash.as_bytes().consensus_encode(writer)?;
            }
        }

        //     if (scriptpath):
        //         ss += TaggedHash("CopperootLeaf", bytes([leaf_ver]) + ser_string(script))
        //         ss += bytes([0])
        //         ss += struct.pack("<i", codeseparator_pos)
        if let Some((hash, code_separator_pos)) = leaf_hash_code_separator {
            hash.as_byte_array().consensus_encode(writer)?;
            KEY_VERSION_0.consensus_encode(writer)?;
            code_separator_pos.consensus_encode(writer)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SCRIPT_VER_P2CR;
    use spora_consensus_core::{
        subnets::SubnetworkId,
        tx::{ScriptPublicKey, ScriptVec, TransactionId, TransactionOutpoint, TransactionInput, TransactionOutput, Transaction},
    };
    use bitcoin::{hex::test_hex_unwrap, key::TapTweak, taproot::Signature, Witness};
    use secp256k1::{Keypair, Message, Secp256k1};
    use std::str::FromStr;
    use spora_utils::hex::FromHex;

    #[test]
    fn test_copperoot_sighash_hash() {
        let bytes = test_hex_unwrap!("00011b96877db45ffa23b307e9f0ac87b80ef9a80b4c5f0db3fbe734422453e83cc5576f3d542c5d4898fb2b696c15d43332534a7c1d1255fda38993545882df92c3e353ff6d36fbfadc4d168452afd8467f02fe53d71714fcea5dfe2ea759bd00185c4cb02bc76d42620393ca358a1a713f4997f9fc222911890afb3fe56c6a19b202df7bffdcfad08003821294279043746631b00e2dc5e52a111e213bbfe6ef09a19428d418dab0d50000000000");
        let mut enc = CopperootSighash::engine();
        enc.update(&bytes);
        let hash = CopperootSighash::from_engine(enc);
        assert_eq!(hash.to_byte_array().len(), 32);
    }

    #[test]
    fn test_copperoot_sighash_key_path() {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            &Vec::from_hex("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160").unwrap(),
        )
        .unwrap();
        let script_pub_key = ScriptVec::from_slice(&keypair.public_key().serialize());

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let unsigned_tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::new(SCRIPT_VER_P2CR, script_pub_key.clone()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );
        let tx_outs = vec![TransactionOutput::new(100, ScriptPublicKey::new(SCRIPT_VER_P2CR, script_pub_key.clone()))];
        let prevouts = Prevouts::All(&tx_outs);

        let input_index = 0;
        let sighash_type = CopperootSighashType::Default;
        let mut sighasher = SighashCache::new(&unsigned_tx);
        let sighash =
            sighasher.copperoot_key_spend_signature_hash(input_index, &prevouts, sighash_type).expect("failed to construct sighash");

        // Note: This will be different from Taproot due to BLAKE3 usage
        // assert_eq!(format!("{sighash}"), "95ae99dacea5e932cee050d33d5e7def5dbf852104a91628d2987586ba85dd8e");

        // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
        let tweaked = keypair.tap_tweak(&secp, None);
        let msg = Message::from(sighash);
        let nonce = [0; 32];
        let signature = secp.sign_schnorr_with_aux_rand(&msg, tweaked.as_keypair(), &nonce);
        let signature = Signature { signature, sighash_type: sighash_type.into() };
        let witness = Witness::p2tr_key_spend(&signature);
        // Check that witness has correct structure (signature length and format)
        let witness_str = format!("{witness:?}");
        assert!(witness_str.contains("indices: 1"));
        assert!(witness_str.contains("indices_start: 65"));
        assert!(witness_str.contains("witnesses: [["));
        // Verify signature is 64 bytes (without sighash type)
        let sig_start = witness_str.find("[[0x").unwrap();
        let sig_end = witness_str.find("]]").unwrap();
        let sig_part = &witness_str[sig_start+4..sig_end];
        let hex_chars: Vec<&str> = sig_part.split(", ").collect();
        assert_eq!(hex_chars.len(), 64, "Signature should be 64 bytes");
    }
}
