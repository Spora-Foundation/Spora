//! psst output structure.

use crate::psst::KeySource;
use crate::utils::combine_if_no_conflicts;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use spora_consensus_core::tx::Script;
use std::{collections::BTreeMap, ops::Add};

#[derive(Builder, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[builder(default)]
pub struct Output {
    /// The Cell capacity in sau.
    pub capacity: u64,
    /// Canonical Cell lock script.
    pub lock_script: Script,
    /// Canonical Cell type script.
    #[builder(setter(strip_option))]
    pub type_script: Option<Script>,
    /// Canonical Cell output data.
    #[builder(setter(strip_option))]
    #[serde(with = "spora_utils::serde_bytes_optional")]
    pub output_data: Option<Vec<u8>>,
    #[builder(setter(strip_option))]
    #[serde(with = "spora_utils::serde_bytes_optional")]
    /// Optional witness template bytes attached to this output metadata.
    pub witness_template: Option<Vec<u8>>,
    /// A map from public keys needed to spend this output to their
    /// corresponding master key fingerprints and derivation paths.
    pub bip32_derivations: BTreeMap<secp256k1::PublicKey, Option<KeySource>>,
    /// Proprietary key-value pairs for this output.
    pub proprietaries: BTreeMap<String, serde_value::Value>,
    #[serde(flatten)]
    /// Unknown key-value pairs for this output.
    pub unknowns: BTreeMap<String, serde_value::Value>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            capacity: 0,
            lock_script: Script::new([0; 32], 0, vec![]),
            type_script: None,
            output_data: None,
            witness_template: None,
            bip32_derivations: BTreeMap::new(),
            proprietaries: BTreeMap::new(),
            unknowns: BTreeMap::new(),
        }
    }
}

impl Add for Output {
    type Output = Result<Self, CombineError>;

    fn add(mut self, rhs: Self) -> Self::Output {
        if self.capacity != rhs.capacity {
            return Err(CombineError::CapacityMismatch { this: self.capacity, that: rhs.capacity });
        }
        if self.lock_script != rhs.lock_script {
            return Err(CombineError::LockScriptMismatch { this: self.lock_script, that: rhs.lock_script });
        }
        if self.type_script != rhs.type_script {
            return Err(CombineError::TypeScriptMismatch { this: self.type_script, that: rhs.type_script });
        }
        if self.output_data != rhs.output_data {
            return Err(CombineError::OutputDataMismatch { this: self.output_data, that: rhs.output_data });
        }
        self.witness_template = match (self.witness_template.take(), rhs.witness_template) {
            (None, None) => None,
            (Some(script), None) | (None, Some(script)) => Some(script),
            (Some(script_left), Some(script_right)) if script_left == script_right => Some(script_left),
            (Some(script_left), Some(script_right)) => {
                return Err(CombineError::NotCompatibleWitnessTemplates { this: script_left, that: script_right })
            }
        };
        self.bip32_derivations = combine_if_no_conflicts(self.bip32_derivations, rhs.bip32_derivations)?;
        self.proprietaries =
            combine_if_no_conflicts(self.proprietaries, rhs.proprietaries).map_err(CombineError::NotCompatibleProprietary)?;
        self.unknowns = combine_if_no_conflicts(self.unknowns, rhs.unknowns).map_err(CombineError::NotCompatibleUnknownField)?;

        Ok(self)
    }
}

/// Error combining two output maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The capacities are not the same")]
    CapacityMismatch { this: u64, that: u64 },
    #[error("The lock scripts are not the same")]
    LockScriptMismatch { this: Script, that: Script },
    #[error("The type scripts are not the same")]
    TypeScriptMismatch { this: Option<Script>, that: Option<Script> },
    #[error("The output data is not the same")]
    OutputDataMismatch { this: Option<Vec<u8>>, that: Option<Vec<u8>> },
    #[error("Two different witness templates detected")]
    NotCompatibleWitnessTemplates { this: Vec<u8>, that: Vec<u8> },

    #[error("Two different derivations for the same key")]
    NotCompatibleBip32Derivations(#[from] crate::utils::Error<secp256k1::PublicKey, Option<KeySource>>),
    #[error("Two different unknown field values")]
    NotCompatibleUnknownField(crate::utils::Error<String, serde_value::Value>),
    #[error("Two different proprietary values")]
    NotCompatibleProprietary(crate::utils::Error<String, serde_value::Value>),
}
