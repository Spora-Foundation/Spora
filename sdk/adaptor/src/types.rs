// In crypto/adaptor/src/types.rs

use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

/// A serializable 32-byte representation of a secp256k1 Scalar.
pub type Scalar32 = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptorSecret(pub Scalar32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptorPoint(pub XOnlyPublicKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptorPartialSignature(pub Scalar32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptorProof {
    pub t: XOnlyPublicKey,
    pub z: Scalar32,
}

// --- Default Implementations for easy testing and initialization ---

impl Default for AdaptorSecret {
    fn default() -> Self { Self([1; 32]) }
}

impl Default for AdaptorPoint {
    fn default() -> Self {
        // 使用一个已知的、可解析的公钥作为默认值
        Self(XOnlyPublicKey::from_slice(&[
            0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
            0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3A, 0xC0,
        ]).unwrap())
    }
}

impl Default for AdaptorPartialSignature {
    fn default() -> Self { Self([0; 32]) } // Partial signature 可以是零
}

impl Default for AdaptorProof {
    fn default() -> Self {
        Self {
            t: AdaptorPoint::default().0,
            z: [1; 32],
        }
    }
}