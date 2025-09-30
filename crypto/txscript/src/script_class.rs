use crate::{opcodes, MAX_SCRIPT_PUBLIC_KEY_VERSION};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use thiserror::Error;
use tondi_addresses::Version;
use tondi_consensus_core::tx::{ScriptPublicKey, ScriptPublicKeyVersion};

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
            ScriptClass::NonStandard => 0,
            ScriptClass::PubKey => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::PubKeyECDSA => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::ScriptHash => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::Taproot => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::CopperootMerkle => MAX_SCRIPT_PUBLIC_KEY_VERSION,
            ScriptClass::CopperootVerkle => MAX_SCRIPT_PUBLIC_KEY_VERSION,
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
        if script_public_key.version() == MAX_SCRIPT_PUBLIC_KEY_VERSION {
            let script = script_public_key.script();
            if Self::is_pay_to_pubkey(script) {
                ScriptClass::PubKey
            } else if Self::is_pay_to_pubkey_ecdsa(script) {
                Self::PubKeyECDSA
            } else if Self::is_pay_to_script_hash(script) {
                Self::ScriptHash
            } else if Self::is_pay_to_taproot(script) {
                // NOTE: Taproot and Copperoot share identical ScriptPubKey format
                // Default to Taproot for backward compatibility
                // Copperoot detection requires additional context (e.g., witness version)
                Self::Taproot
            } else {
                ScriptClass::NonStandard
            }
        } else {
            ScriptClass::NonStandard
        }
    }
}

#[cfg(test)]
mod tests {
    use tondi_consensus_core::tx::ScriptVec;

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
                version: 0,
                class: ScriptClass::PubKey,
            },
            Test {
                name: "valid pubkey ecdsa script",
                script: hex::decode("21fd4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ab").unwrap(),
                version: 0,
                class: ScriptClass::PubKeyECDSA,
            },
            Test {
                name: "valid scripthash script",
                script: hex::decode("aa204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f815187").unwrap(),
                version: 0,
                class: ScriptClass::ScriptHash,
            },
            Test {
                name: "non standard script (unexpected version)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ac").unwrap(),
                version: MAX_SCRIPT_PUBLIC_KEY_VERSION + 1,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected key len)",
                script: hex::decode("1f4a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f81ac").unwrap(),
                version: 0,
                class: ScriptClass::NonStandard,
            },
            Test {
                name: "non standard script (unexpected final check sig op)",
                script: hex::decode("204a23f5eef4b2dead811c7efb4f1afbd8df845e804b6c36a4001fc096e13f8151ad").unwrap(),
                version: 0,
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
