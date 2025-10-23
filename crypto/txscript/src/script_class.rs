use crate::{opcodes, SCRIPT_VER_CLASSIC, SCRIPT_VER_COPPEROOT_MERKLE, SCRIPT_VER_COPPEROOT_VERKLE, SCRIPT_VER_TAPROOT};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_addresses::Version;
use spora_consensus_core::tx::{ScriptPublicKey, ScriptPublicKeyVersion};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum Error {
    #[error("Invalid script class {0}")]
    InvalidScriptClass(String),
}

/// Standard classes of script payment in the blockDAG
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ScriptClass {
    /// None of the recognized forms
    NonStandard = 0,
    /// Pay to pubkey
    PubKey,
    /// Pay to pubkey ECDSA
    PubKeyECDSA,
    /// Pay to script hash
    ScriptHash,
    /// Pay to Taproot
    Taproot,
    /// Pay to Copperoot Merkle (P2CR)
    CopperootMerkle,
    /// Pay to Copperoot Verkle (P2CRV) - Currently disabled for mainnet launch
    CopperootVerkle,
}

const NON_STANDARD: &str = "nonstandard";
const PUB_KEY: &str = "pubkey";
const PUB_KEY_ECDSA: &str = "pubkeyecdsa";
const SCRIPT_HASH: &str = "scripthash";
const TAPROOT: &str = "taproot";
const COPPEROOT_MERKLE: &str = "copperootmerkle";
const COPPEROOT_VERKLE: &str = "copperootverkle";

impl ScriptClass {
    // Returns true if the script passed is a pay-to-pubkey
    // transaction, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_pubkey(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 34) && // 2 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpData32) &&
        (script_public_key[33] == opcodes::codes::OpCheckSig)
    }

    // Returns returns true if the script passed is an ECDSA pay-to-pubkey
    /// transaction, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_pubkey_ecdsa(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 35) && // 2 opcodes number + 33 data
        (script_public_key[0] == opcodes::codes::OpData33) &&
        (script_public_key[34] == opcodes::codes::OpCheckSigECDSA)
    }

    /// Returns true if the script is in the standard
    /// pay-to-script-hash (P2SH) format, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_script_hash(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 35) && // 3 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpBlake3) &&
        (script_public_key[1] == opcodes::codes::OpData32) &&
        (script_public_key[34] == opcodes::codes::OpEqual)
    }

    /// Returns true if the script is in the standard
    /// pay-to-taproot (P2TR) format, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_taproot(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 34) && // 2 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpTrue) &&
        (script_public_key[1] == opcodes::codes::OpData32)
    }

    /// Returns true if the script is in the standard
    /// pay-to-copperoot-merkle (P2CR) format, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_copperoot_merkle(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 34) && // 2 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpTrue) &&
        (script_public_key[1] == opcodes::codes::OpData32)
    }

    /// pay-to-copperoot-verkle (P2CRV) format, false otherwise.
    #[inline(always)]
    pub fn is_pay_to_copperoot_verkle(script_public_key: &[u8]) -> bool {
        (script_public_key.len() == 34) && // 2 opcodes number + 32 data
        (script_public_key[0] == opcodes::codes::OpTrue) &&
        (script_public_key[1] == opcodes::codes::OpData32)
    }

    fn as_str(&self) -> &'static str {
        match self {
            ScriptClass::NonStandard => NON_STANDARD,
            ScriptClass::PubKey => PUB_KEY,
            ScriptClass::PubKeyECDSA => PUB_KEY_ECDSA,
            ScriptClass::ScriptHash => SCRIPT_HASH,
            ScriptClass::Taproot => TAPROOT,
            ScriptClass::CopperootMerkle => COPPEROOT_MERKLE,
            ScriptClass::CopperootVerkle => COPPEROOT_VERKLE,
        }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        match self {
            ScriptClass::NonStandard => SCRIPT_VER_CLASSIC, // Avoid bare 0, use explicit constant
            ScriptClass::PubKey => SCRIPT_VER_CLASSIC,
            ScriptClass::PubKeyECDSA => SCRIPT_VER_CLASSIC,
            ScriptClass::ScriptHash => SCRIPT_VER_CLASSIC,
            ScriptClass::Taproot => SCRIPT_VER_TAPROOT,
            ScriptClass::CopperootMerkle => SCRIPT_VER_COPPEROOT_MERKLE,
            ScriptClass::CopperootVerkle => SCRIPT_VER_COPPEROOT_VERKLE,
        }
    }
}

impl Display for ScriptClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScriptClass {
    type Err = Error;

    fn from_str(script_class: &str) -> Result<Self, Self::Err> {
        match script_class {
            NON_STANDARD => Ok(ScriptClass::NonStandard),
            PUB_KEY => Ok(ScriptClass::PubKey),
            PUB_KEY_ECDSA => Ok(ScriptClass::PubKeyECDSA),
            SCRIPT_HASH => Ok(ScriptClass::ScriptHash),
            TAPROOT => Ok(ScriptClass::Taproot),
            COPPEROOT_MERKLE => Ok(ScriptClass::CopperootMerkle),
            COPPEROOT_VERKLE => Ok(ScriptClass::CopperootVerkle),
            _ => Err(Error::InvalidScriptClass(script_class.to_string())),
        }
    }
}

impl TryFrom<&str> for ScriptClass {
    type Error = Error;

    fn try_from(script_class: &str) -> Result<Self, Self::Error> {
        script_class.parse()
    }
}

impl From<Version> for ScriptClass {
    fn from(value: Version) -> Self {
        match value {
            Version::PubKey => ScriptClass::PubKey,
            Version::PubKeyECDSA => ScriptClass::PubKeyECDSA,
            Version::ScriptHash => ScriptClass::ScriptHash,
            Version::Taproot => ScriptClass::Taproot,
            Version::CopperootMerkle => ScriptClass::CopperootMerkle,
            Version::CopperootVerkle => ScriptClass::CopperootVerkle,
        }
    }
}

impl From<&ScriptPublicKey> for ScriptClass {
    fn from(script_public_key: &ScriptPublicKey) -> Self {
        let version = script_public_key.version();

        // Strictly reject unknown script versions - dispatch based on version number only, no byte pattern checking
        match version {
            SCRIPT_VER_CLASSIC => {
                // Legacy script types (PubKey, PubKeyECDSA, ScriptHash)
                // For Legacy versions, still need to check byte patterns to distinguish specific types
                let script = script_public_key.script();
                if Self::is_pay_to_pubkey(script) {
                    ScriptClass::PubKey
                } else if Self::is_pay_to_pubkey_ecdsa(script) {
                    Self::PubKeyECDSA
                } else if Self::is_pay_to_script_hash(script) {
                    Self::ScriptHash
                } else {
                    ScriptClass::NonStandard
                }
            }
            SCRIPT_VER_TAPROOT => {
                // Validate Taproot script format
                let script = script_public_key.script();
                if Self::is_pay_to_taproot(script) {
                    ScriptClass::Taproot
                } else {
                    ScriptClass::NonStandard
                }
            }
            SCRIPT_VER_COPPEROOT_MERKLE => {
                // Validate Copperoot Merkle script format
                let script = script_public_key.script();
                if Self::is_pay_to_copperoot_merkle(script) {
                    ScriptClass::CopperootMerkle
                } else {
                    ScriptClass::NonStandard
                }
            }
            SCRIPT_VER_COPPEROOT_VERKLE => {
                // Validate Copperoot Verkle script format
                let script = script_public_key.script();
                if Self::is_pay_to_copperoot_verkle(script) {
                    ScriptClass::CopperootVerkle
                } else {
                    ScriptClass::NonStandard
                }
            }
            _ => ScriptClass::NonStandard, // Unknown versions are directly marked as non-standard
        }
    }
}

#[cfg(test)]
mod tests {
    use spora_consensus_core::tx::ScriptVec;

    use super::*;

    #[test]
    fn test_script_class_from_script() {
        struct Test {
            name: &'static str,
            script: Vec<u8>,
            version: ScriptPublicKeyVersion,
            class: ScriptClass,
        }

        // cspell:disable
        let tests = vec![
            Test {
                name: "valid pubkey script",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(),
                version: SCRIPT_VER_CLASSIC,
                class: ScriptClass::PubKey,
            },
            Test {
                name: "valid pubkey ecdsa script",
                script: hex::decode("21fd4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ab").unwrap(),
                version: SCRIPT_VER_CLASSIC,
                class: ScriptClass::PubKeyECDSA,
            },
            Test {
                name: "valid scripthash script",
                script: hex::decode("aa204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f815187").unwrap(),
                version: SCRIPT_VER_CLASSIC,
                class: ScriptClass::ScriptHash,
            },
            Test {
                name: "valid taproot script",
                script: hex::decode("51204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151").unwrap(),
                version: SCRIPT_VER_TAPROOT,
                class: ScriptClass::Taproot,
            },
            Test {
                name: "valid copperoot merkle script",
                script: hex::decode("51204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151").unwrap(),
                version: SCRIPT_VER_COPPEROOT_MERKLE,
                class: ScriptClass::CopperootMerkle,
            },
            Test {
                name: "non standard script (unexpected version)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(),
                version: SCRIPT_VER_COPPEROOT_VERKLE + 1, // Use a truly unknown version
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected key len)",
                script: hex::decode("1f4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f81ac").unwrap(),
                version: SCRIPT_VER_CLASSIC,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected final check sig op)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ad").unwrap(),
                version: SCRIPT_VER_CLASSIC,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard taproot script (wrong format)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(), // Wrong format for taproot
                version: SCRIPT_VER_TAPROOT,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard copperoot merkle script (wrong format)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(), // Wrong format for copperoot merkle
                version: SCRIPT_VER_COPPEROOT_MERKLE,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard copperoot verkle script (wrong format)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(), // Wrong format for copperoot verkle
                version: SCRIPT_VER_COPPEROOT_VERKLE,
                class: ScriptClass::NonStandard,
            },
        ];
        // cspell:enable

        for test in tests {
            let script_public_key = ScriptPublicKey::new(test.version, ScriptVec::from_iter(test.script.iter().copied()));
            assert_eq!(test.class, ScriptClass::from(&script_public_key), "{} wrong script class", test.name);
        }
    }
}
