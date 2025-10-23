// adaptor/src/error.rs

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AdaptorError {
    #[error("secp256k1 error: {0}")]
    Secp(#[from] secp256k1::Error),

    #[error("the provided scalar is out of the curve order range")]
    ScalarOutOfRange,

    #[error("the NIZK proof for Y=xG failed verification")]
    ProofVerificationFailed,
}

// 实现 From trait，让 `?` 操作符可以自动转换错误类型
impl From<secp256k1::scalar::OutOfRangeError> for AdaptorError {
    fn from(_: secp256k1::scalar::OutOfRangeError) -> Self {
        AdaptorError::ScalarOutOfRange
    }
}
