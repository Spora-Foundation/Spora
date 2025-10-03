use std::borrow::Borrow;

use bitcoin::{
    consensus::Encodable,
    io::{Error as BitcoinIoError, Write},
    VarInt,
};
use secp256k1::{
    hashes::{sha256, sha256d, sha256t_hash_newtype, Hash},
    Message,
};

use crate::tx::{taproot::error::TaprootError, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};

const KEY_VERSION_0: u8 = 0u8;

sha256t_hash_newtype! {
    pub struct TapSighashTag = hash_str("TapSighash");

    /// Taproot-tagged hash with tag \"TapSighash\".
    ///
    /// This hash type is used for computing taproot signature hash."
    #[hash_newtype(forward)]
    pub struct TapSighash(_);
}

impl From<TapSighash> for Message {
    fn from(hash: TapSighash) -> Self {
        Message::from_digest(hash.to_byte_array())
    }
}

sha256t_hash_newtype! {
    pub struct TapLeafTag = hash_str("TapLeaf");

    /// Taproot-tagged hash with tag \"TapLeaf\".
    ///
    /// This is used for computing tapscript script spend hash.
    #[hash_newtype(forward)]
    pub struct TapLeafHash(_);
}

/// Hashtype of an input's signature, encoded in the last byte of the signature.
/// Fixed values so they can be cast as integer types for encoding.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TapSighashType {
    /// 0x0: Used when not explicitly specified, defaults to [`TapSighashType::All`]
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

impl TapSighashType {
    /// Breaks the sighash flag into the "real" sighash flag and the `SIGHASH_ANYONECANPAY` boolean.
    pub(crate) fn split_anyonecanpay_flag(self) -> (TapSighashType, bool) {
        use TapSighashType::*;

        match self {
            Default => (Default, false),
            All => (All, false),
            None => (None, false),
            Single => (Single, false),
            AllPlusAnyoneCanPay => (All, true),
            NonePlusAnyoneCanPay => (None, true),
            SinglePlusAnyoneCanPay => (Single, true),
        }
    }

    /// Constructs a [`TapSighashType`] from a raw `u8`.
    pub fn from_consensus_u8(sighash_type: u8) -> Result<Self, TaprootError> {
        use TapSighashType::*;

        Ok(match sighash_type {
            0x00 => Default,
            0x01 => All,
            0x02 => None,
            0x03 => Single,
            0x81 => AllPlusAnyoneCanPay,
            0x82 => NonePlusAnyoneCanPay,
            0x83 => SinglePlusAnyoneCanPay,
            _ => return Err(TaprootError::InvalidSighashTypeError),
        })
    }
}

impl From<TapSighashType> for bitcoin::TapSighashType {
    fn from(ty: TapSighashType) -> Self {
        match ty {
            TapSighashType::Default => Self::Default,
            TapSighashType::All => Self::All,
            TapSighashType::None => Self::None,
            TapSighashType::Single => Self::Single,
            TapSighashType::AllPlusAnyoneCanPay => Self::AllPlusAnyoneCanPay,
            TapSighashType::NonePlusAnyoneCanPay => Self::NonePlusAnyoneCanPay,
            TapSighashType::SinglePlusAnyoneCanPay => Self::SinglePlusAnyoneCanPay,
        }
    }
}

/// The `Annex` struct is a slice wrapper enforcing first byte is `0x50`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Annex<'a>(&'a [u8]);

impl<'a> Encodable for Annex<'a> {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, BitcoinIoError> {
        let data = self.0;
        let vi_len = VarInt(data.len() as u64).consensus_encode(w)?;
        w.write_all(data)?;
        Ok(vi_len + data.len())
    }
}

/// Contains outputs of previous transactions. In the case [`TapSighashType`] variant is
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
    fn check_all(&self, tx: &Transaction) -> Result<(), TaprootError> {
        if let Prevouts::All(prevouts) = self {
            if prevouts.len() != tx.inputs.len() {
                return Err(TaprootError::PrevoutsError);
            }
        }
        Ok(())
    }

    fn get_all(&self) -> Result<&[TxOut], TaprootError> {
        match self {
            Prevouts::All(prevouts) => Ok(*prevouts),
            _ => Err(TaprootError::PrevoutsError),
        }
    }

    fn get(&self, input_index: usize) -> Result<&TransactionOutput, TaprootError> {
        match self {
            Prevouts::One(index, prevout) => {
                if input_index == *index {
                    Ok(prevout.borrow())
                } else {
                    Err(TaprootError::PrevoutsError)
                }
            }
            Prevouts::All(prevouts) => prevouts.get(input_index).map(|x| x.borrow()).ok_or(TaprootError::PrevoutsError),
        }
    }
}

/// Common values cached between segwit and taproot inputs.
#[allow(dead_code)]
#[derive(Debug)]
struct CommonCache {
    prevouts: sha256::Hash,
    sequences: sha256::Hash,

    /// In theory `outputs` could be an `Option` since `SIGHASH_NONE` and `SIGHASH_SINGLE` do not
    /// need it, but since `SIGHASH_ALL` is by far the most used variant we don't bother.
    outputs: sha256::Hash,
}

/// Values cached for segwit inputs, equivalent to [`CommonCache`] plus another round of `sha256`.
#[allow(dead_code)]
#[derive(Debug)]
struct SegwitCache {
    prevouts: sha256d::Hash,
    sequences: sha256d::Hash,
    outputs: sha256d::Hash,
}

/// Values cached for taproot inputs.
#[allow(dead_code)]
#[derive(Debug)]
struct TaprootCache {
    amounts: sha256::Hash,
    script_pubkeys: sha256::Hash,
}

/// Efficiently calculates signature hash message for legacy, segwit and taproot inputs.
#[derive(Debug)]
pub struct SighashCache<Tx: Borrow<Transaction>> {
    /// Access to transaction required for transaction introspection. Moreover, type
    /// `T: Borrow<Transaction>` allows us to use borrowed and mutable borrowed types,
    /// the latter in particular is necessary for [`SighashCache::witness_mut`].
    tx: Tx,
    /// Common cache for taproot and segwit inputs, `None` for legacy inputs.
    common_cache: Option<CommonCache>,
    #[allow(dead_code)]
    /// Cache for segwit v0 inputs (the result of another round of sha256 on `common_cache`).
    segwit_cache: Option<SegwitCache>,
    /// Cache for taproot v1 inputs.
    taproot_cache: Option<TaprootCache>,
}

impl Encodable for TransactionOutpoint {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, BitcoinIoError> {
        let len = self.transaction_id.as_bytes().consensus_encode(w)?;
        Ok(len + self.index.consensus_encode(w)?)
    }
}

impl Encodable for TransactionOutput {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, BitcoinIoError> {
        let mut len = 0;
        len += self.value.consensus_encode(w)?;
        // FIXME: ScriptPublicKey.version
        len += self.script_public_key.script().to_vec().consensus_encode(w)?;
        Ok(len)
    }
}

impl<Tx: Borrow<Transaction>> SighashCache<Tx> {
    /// Constructs a new `SighashCache` from an unsigned transaction.
    ///
    /// The sighash components are computed in a lazy manner when required. For the generated
    /// sighashes to be valid, no fields in the transaction may change except for script_sig and
    /// witness.
    pub fn new(tx: Tx) -> Self {
        SighashCache { tx, common_cache: None, taproot_cache: None, segwit_cache: None }
    }

    #[inline]
    fn common_cache(&mut self) -> &CommonCache {
        Self::common_cache_minimal_borrow(&mut self.common_cache, self.tx.borrow())
    }

    fn taproot_cache<TxOut: Borrow<TransactionOutput>>(&mut self, prevouts: &[TxOut]) -> &TaprootCache {
        self.taproot_cache.get_or_insert_with(|| {
            let mut enc_amounts = sha256::Hash::engine();
            let mut enc_script_pubkeys = sha256::Hash::engine();
            for prevout in prevouts {
                let txout = prevout.borrow();
                txout.value.consensus_encode(&mut enc_amounts).unwrap();
                txout.script_public_key.script().to_vec().consensus_encode(&mut enc_script_pubkeys).unwrap();
            }
            TaprootCache {
                amounts: sha256::Hash::from_engine(enc_amounts),
                script_pubkeys: sha256::Hash::from_engine(enc_script_pubkeys),
            }
        })
    }

    fn common_cache_minimal_borrow<'a>(common_cache: &'a mut Option<CommonCache>, tx: &Transaction) -> &'a CommonCache {
        common_cache.get_or_insert_with(|| {
            let mut enc_prevouts = sha256::Hash::engine();
            let mut enc_sequences = sha256::Hash::engine();
            for txin in tx.inputs.iter() {
                txin.previous_outpoint.consensus_encode(&mut enc_prevouts).unwrap();
                txin.sequence.consensus_encode(&mut enc_sequences).unwrap();
            }
            CommonCache {
                prevouts: sha256::Hash::from_engine(enc_prevouts),
                sequences: sha256::Hash::from_engine(enc_sequences),
                outputs: {
                    let mut enc = sha256::Hash::engine();
                    for txout in tx.outputs.iter() {
                        txout.consensus_encode(&mut enc).unwrap();
                    }
                    sha256::Hash::from_engine(enc)
                },
            }
        })
    }

    /// Computes the BIP341 sighash for a key spend.
    pub fn taproot_key_spend_signature_hash<TxOut: Borrow<TransactionOutput>>(
        &mut self,
        input_index: usize,
        prevouts: &Prevouts<TxOut>,
        sighash_type: TapSighashType,
    ) -> Result<TapSighash, TaprootError> {
        let mut enc = TapSighash::engine();
        self.taproot_encode_signing_data_to(&mut enc, input_index, prevouts, None, None, sighash_type)?;
        Ok(TapSighash::from_engine(enc))
    }

    /// Encodes the BIP341 signing data for any flag type into a given object implementing the
    /// [`io::Write`] trait.
    pub fn taproot_encode_signing_data_to<W: Write + ?Sized, TxOut: Borrow<TransactionOutput>>(
        &mut self,
        writer: &mut W,
        input_index: usize,
        prevouts: &Prevouts<TxOut>,
        annex: Option<Annex>,
        leaf_hash_code_separator: Option<(TapLeafHash, u32)>,
        sighash_type: TapSighashType,
    ) -> Result<(), TaprootError> {
        prevouts.check_all(self.tx.borrow())?;

        let (sighash, anyone_can_pay) = sighash_type.split_anyonecanpay_flag();

        // epoch
        0u8.consensus_encode(writer)?;

        // * Control:
        // hash_type (1).
        (sighash_type as u8).consensus_encode(writer)?;

        // * Transaction Data:
        // nVersion (4): the nVersion of the transaction.
        self.tx.borrow().version.consensus_encode(writer)?;

        // nLockTime (4): the nLockTime of the transaction.
        self.tx.borrow().lock_time.consensus_encode(writer)?;

        // If the hash_type & 0x80 does not equal SIGHASH_ANYONECANPAY:
        //     sha_prevouts (32): the SHA256 of the serialization of all input outpoints.
        //     sha_amounts (32): the SHA256 of the serialization of all spent output amounts.
        //     sha_scriptpubkeys (32): the SHA256 of the serialization of all spent output scriptPubKeys.
        //     sha_sequences (32): the SHA256 of the serialization of all input nSequence.
        if !anyone_can_pay {
            self.common_cache().prevouts.consensus_encode(writer)?;
            self.taproot_cache(prevouts.get_all()?).amounts.consensus_encode(writer)?;
            self.taproot_cache(prevouts.get_all()?).script_pubkeys.consensus_encode(writer)?;
            self.common_cache().sequences.consensus_encode(writer)?;
        }

        // If hash_type & 3 does not equal SIGHASH_NONE or SIGHASH_SINGLE:
        //     sha_outputs (32): the SHA256 of the serialization of all outputs in CTxOut format.
        if sighash != TapSighashType::None && sighash != TapSighashType::Single {
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
        //      scriptPubKey (35): scriptPubKey of the previous output spent by this input, serialized as script inside CTxOut. Its size is always 35 bytes.
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
        //      sha_annex (32): the SHA256 of (compact_size(size of annex) || annex), where annex
        //      includes the mandatory 0x50 prefix.
        if let Some(annex) = annex {
            let mut enc = sha256::Hash::engine();
            annex.consensus_encode(&mut enc)?;
            let hash = sha256::Hash::from_engine(enc);
            hash.consensus_encode(writer)?;
        }

        // * Data about this output:
        // If hash_type & 3 equals SIGHASH_SINGLE:
        //      sha_single_output (32): the SHA256 of the corresponding output in CTxOut format.
        if sighash == TapSighashType::Single {
            let mut enc = sha256::Hash::engine();
            self.tx.borrow().outputs[input_index].consensus_encode(&mut enc)?;
            let hash = sha256::Hash::from_engine(enc);
            hash.consensus_encode(writer)?;
        }

        //     if (scriptpath):
        //         ss += TaggedHash("TapLeaf", bytes([leaf_ver]) + ser_string(script))
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
    use crate::{
        constants::SCRIPT_VER_TAPROOT,
        subnets::SubnetworkId,
        tx::{ScriptPublicKey, ScriptVec, TransactionId},
    };
    use bitcoin::{hashes::HashEngine, hex::test_hex_unwrap, key::TapTweak, taproot::Signature, Witness};
    use secp256k1::{Keypair, Message, Secp256k1};
    use std::str::FromStr;
    use tondi_utils::hex::FromHex;

    #[test]
    fn test_tap_sighash_hash() {
        let bytes = test_hex_unwrap!("00011b96877db45ffa23b307e9f0ac87b80ef9a80b4c5f0db3fbe734422453e83cc5576f3d542c5d4898fb2b696c15d43332534a7c1d1255fda38993545882df92c3e353ff6d36fbfadc4d168452afd8467f02fe53d71714fcea5dfe2ea759bd00185c4cb02bc76d42620393ca358a1a713f4997f9fc222911890afb3fe56c6a19b202df7bffdcfad08003821294279043746631b00e2dc5e52a111e213bbfe6ef09a19428d418dab0d50000000000");
        let expected = test_hex_unwrap!("04e808aad07a40b3767a1442fead79af6ef7e7c9316d82dec409bb31e77699b0");
        let mut enc = TapSighash::engine();
        enc.input(&bytes);
        let hash = TapSighash::from_engine(enc);
        assert_eq!(expected, hash.to_byte_array());
    }

    #[test]
    fn test_tap_sighash_key_path() {
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
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::new(SCRIPT_VER_TAPROOT, script_pub_key.clone()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );
        let txOuts = vec![TransactionOutput::new(100, ScriptPublicKey::new(SCRIPT_VER_TAPROOT, script_pub_key.clone()))];
        let prevouts = Prevouts::All(&txOuts);

        let input_index = 0;
        let sighash_type = TapSighashType::Default;
        let mut sighasher = SighashCache::new(&unsigned_tx);
        let sighash =
            sighasher.taproot_key_spend_signature_hash(input_index, &prevouts, sighash_type).expect("failed to construct sighash");

        assert_eq!(format!("{sighash}"), "95ae99dacea5e932cee050d33d5e7def5dbf852104a91628d2987586ba85dd8e");

        // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
        let tweaked = keypair.tap_tweak(&secp, None);
        let msg = Message::from(sighash);
        let nonce = [0; 32];
        let signature = secp.sign_schnorr_with_aux_rand(&msg, tweaked.as_keypair(), &nonce);
        let signature = Signature { signature, sighash_type: sighash_type.into() };
        let witness = Witness::p2tr_key_spend(&signature);
        assert_eq!(format!("{witness:?}"), "Witness: { indices: 1, indices_start: 65, witnesses: [[0x06, 0xbe, 0xc9, 0xe0, 0x29, 0xf7, 0xb7, 0x78, 0xf0, 0x56, 0xfa, 0x3f, 0xe2, 0x36, 0xed, 0x07, 0xdb, 0x84, 0x23, 0xa2, 0x69, 0x56, 0x93, 0x14, 0xd0, 0x58, 0x48, 0x61, 0x41, 0x39, 0x64, 0x04, 0xd7, 0xa6, 0x3d, 0xca, 0xb8, 0xc3, 0x7b, 0xf5, 0xdd, 0x95, 0x9d, 0xdc, 0x55, 0x2d, 0x71, 0x49, 0x51, 0xf8, 0xa3, 0x78, 0xf5, 0x83, 0x75, 0xf4, 0x95, 0x23, 0xb5, 0x69, 0x23, 0x8a, 0x6e, 0x40]] }")
    }
}
