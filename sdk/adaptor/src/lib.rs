// In crypto/adaptor/src/lib.rs

//! Adaptor Signatures for Scriptless Scripts.

pub mod error;
pub mod types;

pub use error::AdaptorError;
pub use types::{
    AdaptorPartialSignature, AdaptorPoint, AdaptorProof, AdaptorSecret, Scalar32,
};

use k256::{
    elliptic_curve::{generic_array::GenericArray, ops::Reduce, PrimeField},
    Scalar as KScalar, U256,
};
use secp256k1::{
    self, rand::rngs::OsRng, All, Keypair, Scalar, Secp256k1, SecretKey,
};
use sha2::{Digest, Sha256};

// --- Helper functions ---
fn kscalar_from_bytes(bytes: &[u8; 32]) -> KScalar {
    <KScalar as Reduce<U256>>::reduce_bytes(&GenericArray::from(*bytes))
}

fn kscalar_to_bytes(scalar: &KScalar) -> [u8; 32] {
    scalar.to_repr().into()
}

fn secp_scalar_from_bytes(bytes: &[u8; 32]) -> Result<Scalar, AdaptorError> {
    Scalar::from_be_bytes(*bytes).map_err(|e| e.into())
}

// --- Core API ---

pub fn create_secret_and_point(secp: &Secp256k1<All>) -> (AdaptorSecret, AdaptorPoint) {
    let mut rng = rand::thread_rng();
    let keypair = Keypair::new(secp, &mut rng);

    let mut secret_key = keypair.secret_key();
    if keypair.public_key().x_only_public_key().1 == secp256k1::Parity::Odd {
        secret_key = secret_key.negate();
    }

    let final_public_key = secret_key.public_key(secp);
    let point_y = AdaptorPoint(final_public_key.x_only_public_key().0);

    (AdaptorSecret(secret_key.secret_bytes()), point_y)
}

pub fn create_proof(
    secp: &Secp256k1<All>,
    secret_x_bytes: &Scalar32,
    point_y: &AdaptorPoint,
) -> Result<AdaptorProof, AdaptorError> {
    let mut rng = OsRng;
    let x = kscalar_from_bytes(secret_x_bytes);

    let k_sk = SecretKey::new(&mut rng);
    let k = kscalar_from_bytes(&k_sk.secret_bytes());
    let t_point = k_sk.public_key(secp);
    let (t_xonly, _) = t_point.x_only_public_key();

    let mut hasher = Sha256::new();
    hasher.update(point_y.0.serialize());
    hasher.update(t_xonly.serialize());
    let e_bytes: [u8; 32] = hasher.finalize().into();
    let e = kscalar_from_bytes(&e_bytes);

    let z_kscalar = k + (e * x);
    let z_bytes = kscalar_to_bytes(&z_kscalar);

    Ok(AdaptorProof { t: t_xonly, z: z_bytes })
}

pub fn verify_proof(
    secp: &Secp256k1<All>,
    point_y: &AdaptorPoint,
    proof: &AdaptorProof,
) -> Result<(), AdaptorError> {
    let y_point = point_y.0.public_key(secp256k1::Parity::Even);

    let mut hasher = Sha256::new();
    hasher.update(point_y.0.serialize());
    hasher.update(proof.t.serialize());
    let e_bytes: [u8; 32] = hasher.finalize().into();
    let e_scalar = secp_scalar_from_bytes(&e_bytes)?;

    let e_sk = SecretKey::from_slice(&e_scalar.to_be_bytes())?;
    let neg_e_sk = e_sk.negate();
    let neg_e_scalar = Scalar::from(neg_e_sk);

    let neg_ey_point = y_point.mul_tweak(secp, &neg_e_scalar)?;

    let z_sk = SecretKey::from_slice(&proof.z)?;
    let zg_point = z_sk.public_key(secp);

    let lhs_point = zg_point.combine(&neg_ey_point)?;
    let (lhs_xonly, _) = lhs_point.x_only_public_key();

    if lhs_xonly == proof.t { Ok(()) }
    else { Err(AdaptorError::ProofVerificationFailed) }
}


pub fn make_partial_signature(
    s_scalar: &Scalar,
    x_bytes: &Scalar32,
) -> Result<AdaptorPartialSignature, AdaptorError> {
    let s_k = kscalar_from_bytes(&s_scalar.to_be_bytes());
    let x_k = kscalar_from_bytes(x_bytes);
    let s_prime_k = s_k - x_k;
    Ok(AdaptorPartialSignature(kscalar_to_bytes(&s_prime_k)))
}

pub fn complete_signature(
    s_prime_bytes: &Scalar32,
    x_bytes: &Scalar32,
) -> Result<Scalar, AdaptorError> {
    let s_prime_k = kscalar_from_bytes(s_prime_bytes);
    let x_k = kscalar_from_bytes(x_bytes);
    let s_k = s_prime_k + x_k;
    secp_scalar_from_bytes(&kscalar_to_bytes(&s_k))
}

pub fn recover_secret(
    s_bytes: &Scalar32,
    s_prime_bytes: &Scalar32,
) -> Result<Scalar32, AdaptorError> {
    let s_k = kscalar_from_bytes(s_bytes);
    let s_prime_k = kscalar_from_bytes(s_prime_bytes);
    let x_k = s_k - s_prime_k;
    Ok(kscalar_to_bytes(&x_k))
}
/// Verifies that a recovered secret `x` corresponds to the adaptor point `Y`.
/// Checks `x * G == Y`.
pub fn verify_secret(
    secp: &Secp256k1<All>,
    secret_x: &AdaptorSecret,
    point_y: &AdaptorPoint,
) -> bool {
    // 从秘密字节创建私钥
    if let Ok(sk) = SecretKey::from_slice(&secret_x.0) {
        // 计算 x * G
        let (pk_xonly, _) = sk.public_key(secp).x_only_public_key();
        // 比较计算出的公钥与 Y 点是否相等
        pk_xonly == point_y.0
    } else {
        false
    }
}
/// A public helper function to multiply two secp256k1 Scalars using k256 for the arithmetic.
pub fn multiply_scalars(a: &Scalar, b: &Scalar) -> Result<Scalar, AdaptorError> {
    let a_k = kscalar_from_bytes(&a.to_be_bytes());
    let b_k = kscalar_from_bytes(&b.to_be_bytes());
    let result_k = a_k * b_k;
    secp_scalar_from_bytes(&kscalar_to_bytes(&result_k))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptor_signature_full_roundtrip() {
        let secp = Secp256k1::new();
        let mut rng = OsRng;

        let (x_secret, y_point) = create_secret_and_point(&secp);
        let proof = create_proof(&secp, &x_secret.0, &y_point).expect("Proof generation failed");
        verify_proof(&secp, &y_point, &proof).expect("Alice failed to verify Bob's proof");

        let s_scalar = Scalar::from(SecretKey::new(&mut rng));
        let s_bytes = s_scalar.to_be_bytes();

        let s_prime = make_partial_signature(&s_scalar, &x_secret.0).expect("Failed to make partial");
        let completed_s = complete_signature(&s_prime.0, &x_secret.0).expect("Failed to complete");

        assert_eq!(s_scalar, completed_s);

        let recovered_x = recover_secret(&s_bytes, &s_prime.0).expect("Failed to recover secret");
        assert_eq!(x_secret.0, recovered_x);

        let recovered_sk = SecretKey::from_slice(&recovered_x).unwrap();
        let (recovered_y, _) = recovered_sk.public_key(&secp).x_only_public_key();
        assert_eq!(y_point.0, recovered_y);
        println!("Adaptor signature roundtrip test passed with serializable types!");
    }
}