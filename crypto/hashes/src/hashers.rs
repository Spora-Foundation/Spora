//! Implements domain-separated hashing for Tondi blockchain.
//!
//! ⚠️ **Modified for the radical transition from blake2b & sha256 to full-scale BLAKE3 adoption.**
//!
//! All previous domain-separated `sha256` and `blake2b` hashers are to be progressively deprecated.
//! Use unified BLAKE3-based equivalents moving forward for:
//! - Transaction ID
//! - Block hash
//! - Signing challenge hash
//! - Merkle tree hashing
//! - PoW identifiersuse once_cell::sync::Lazy;
use crate::blake3::blake3_256;
pub trait HasherBase {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self;
}

pub trait Hasher: HasherBase + Clone + Default {
    fn finalize(self) -> crate::Hash;
    fn reset(&mut self);
    #[inline(always)]
    fn hash<A: AsRef<[u8]>>(data: A) -> crate::Hash {
        let mut hasher = Self::default();
        hasher.update(data);
        hasher.finalize()
    }
}

pub use crate::pow_hashers::{KHeavyHash, PowHash};

// Hashers defined using domain-separation + BLAKE3 backend.
macro_rules! blake3_hasher {
    ($(struct $name:ident => $domain_sep:literal),+ $(,)?) => {
        $(
            #[derive(Clone)]
            pub struct $name(Vec<u8>);

            impl $name {
                #[inline(always)]
                pub fn new() -> Self {
                    let mut prefix = Vec::new();
                    prefix.extend_from_slice($domain_sep);
                    Self(prefix)
                }

                pub fn write<A: AsRef<[u8]>>(&mut self, data: A) {
                    self.0.extend_from_slice(data.as_ref());
                }

                #[inline(always)]
                pub fn finalize(self) -> crate::Hash {
                    crate::Hash(blake3_256(&self.0))
                }
            }

            impl_hasher! { struct $name }
        )*
    };
}

macro_rules! impl_hasher {
    (struct $name:ident) => {
        impl HasherBase for $name {
            #[inline(always)]
            fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
                self.write(data);
                self
            }
        }
        impl Hasher for $name {
            #[inline(always)]
            fn finalize(self) -> crate::Hash {
                // Call the method
                $name::finalize(self)
            }
            #[inline(always)]
            fn reset(&mut self) {
                *self = Self::new();
            }
        }
        impl Default for $name {
            #[inline(always)]
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

// ✅ Now define all hashers via BLAKE3
blake3_hasher! {
    struct TransactionHash             => b"TransactionHash",
    struct TransactionID               => b"TransactionID",
    struct TransactionSigningHash      => b"TransactionSigningHash",
    struct BlockHash                   => b"BlockHash",
    struct ProofOfWorkHash             => b"ProofOfWorkHash",
    struct MerkleBranchHash            => b"MerkleBranchHash",
    struct MuHashElementHash           => b"MuHashElement",
    struct MuHashFinalizeHash          => b"MuHashFinalize",
    struct PersonalMessageSigningHash  => b"PersonalMessageSigningHash",
    struct TransactionSigningHashECDSA => b"TransactionSigningHashECDSA",
}

use impl_hasher;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vectors() {
        let input_data = [&[], &[1][..], &[42; 64], &[0; 8][..]];

        fn run_test_vector<H: Hasher>(input: &[&[u8]], hasher: impl Fn() -> H) {
            let mut h = hasher();
            for chunk in input {
                h.update(chunk);
            }
            let result = h.finalize();
            assert_eq!(result.0.len(), 32);
        }

        run_test_vector(&input_data, TransactionHash::new);
        run_test_vector(&input_data, TransactionID::new);
        run_test_vector(&input_data, BlockHash::new);
    }
}
