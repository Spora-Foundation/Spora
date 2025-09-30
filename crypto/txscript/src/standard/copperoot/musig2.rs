//! MuSig2 wrapper for Copperoot using standard musig2 crate
//!
//! This module provides a simplified wrapper around the musig2 crate,
//! using standard BIP340 hashing (SHA256) instead of BLAKE3.

// Re-export types from musig2 crate
pub use musig2::{
    KeyAggContext as MuSig2KeyAgg,
    FirstRound, SecondRound, PartialSignature, CompactSignature,
    errors::{KeyAggError, SigningError, VerifyError},
    NonceSeed, SecNonceSpices, PubNonce,
};

use secp256k1::{Keypair, Secp256k1, XOnlyPublicKey, All};
use std::collections::HashMap;
use tondi_hashes::Hash;

/// MuSig2 nonce wrapper (for compatibility)
#[derive(Debug, Clone)]
pub struct MuSig2Nonce {
    /// Public nonce
    pub public_nonce: PubNonce,
}

/// MuSig2 session wrapper (for compatibility)
#[allow(dead_code)]
pub struct MuSig2Session {
    /// First round context (private since FirstRound doesn't impl Debug)
    first_round: FirstRound,
    /// Nonces from all participants
    pub nonces: HashMap<usize, MuSig2Nonce>,
    /// Session ID
    pub session_id: Hash,
}

/// MuSig2 signature wrapper (for compatibility)
#[derive(Debug, Clone)]
pub struct MuSig2Signature {
    /// Internal signature
    pub inner: CompactSignature,
    /// Aggregated public key
    pub aggregated_key: XOnlyPublicKey,
}

/// Encrypted signature (placeholder for future implementation)
#[derive(Debug, Clone)]
pub struct EncryptedSignature {
    /// Placeholder data
    pub data: Vec<u8>,
}

impl MuSig2Nonce {
    /// Generate a new nonce (simplified interface)
    pub fn new(_secp: &Secp256k1<All>, _keypair: &Keypair, _message: &[u8]) -> Result<Self, MuSig2Error> {
        // This is a simplified placeholder - in practice, you'd generate a proper nonce
        // The actual nonce generation happens in FirstRound::new()
        Err(MuSig2Error::NonceGenerationFailed)
    }
}

impl MuSig2Session {
    /// Create a new MuSig2 session (simplified interface)
    pub fn new(
        _key_agg: MuSig2KeyAgg,
        nonces: HashMap<usize, MuSig2Nonce>,
    ) -> Result<Self, MuSig2Error> {
        if nonces.is_empty() {
            return Err(MuSig2Error::EmptyNonceList);
        }

        // This is a placeholder - the actual session creation requires more parameters
        // In practice, you'd use FirstRound::new() with proper parameters
        Err(MuSig2Error::SessionCreationFailed)
    }

    /// Sign a message (simplified interface)
    pub fn sign(
        &self,
        _secp: &Secp256k1<All>,
        _keypair: &Keypair,
        _message: &[u8],
        _participant_index: usize,
    ) -> Result<MuSig2Signature, MuSig2Error> {
        // This is a placeholder - actual signing requires SecondRound
        Err(MuSig2Error::SigningFailed)
    }
}

/// MuSig2 errors
#[derive(Debug, thiserror::Error)]
pub enum MuSig2Error {
    #[error("Empty key list")]
    EmptyKeyList,
    #[error("Empty nonce list")]
    EmptyNonceList,
    #[error("Invalid participant index")]
    InvalidParticipantIndex,
    #[error("Key aggregation failed")]
    KeyAggregationFailed,
    #[error("Nonce generation failed")]
    NonceGenerationFailed,
    #[error("Session creation failed")]
    SessionCreationFailed,
    #[error("Signing failed")]
    SigningFailed,
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
    #[error("Invalid signature")]
    InvalidSignature,
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_musig2_key_aggregation() {
        use musig2::secp256k1::{Secp256k1 as MuSecp, Keypair, PublicKey as MuPubKey, SecretKey};
        use musig2::KeyAggContext;
        use secp256k1::rand;
        use rand::Rng;

        let secp = MuSecp::new();
        let mut rng = rand::thread_rng();
        
        // Create secret keys and keypairs
        let mut sk1_bytes = [0u8; 32];
        let mut sk2_bytes = [0u8; 32];
        rng.fill(&mut sk1_bytes);
        rng.fill(&mut sk2_bytes);
        
        let sk1 = SecretKey::from_byte_array(sk1_bytes).expect("valid secret key");
        let sk2 = SecretKey::from_byte_array(sk2_bytes).expect("valid secret key");
        let kp1 = Keypair::from_secret_key(&secp, &sk1);
        let kp2 = Keypair::from_secret_key(&secp, &sk2);

        // Get public keys
        let pubkeys = vec![MuPubKey::from(kp1), MuPubKey::from(kp2)];
        let key_agg = KeyAggContext::new(pubkeys).expect("key aggregation failed");

        let agg_pk: MuPubKey = key_agg.aggregated_pubkey();
        assert_eq!(agg_pk.serialize().len(), 33);
    }

    #[test]
    fn test_musig2_two_party_sign() {
        use musig2::secp256k1::{
            Secp256k1 as MuSecp, Keypair, PublicKey as MuPubKey, SecretKey
        };
        use musig2::{KeyAggContext, FirstRound, SecNonceSpices};
        use secp256k1::rand;
        use rand::Rng;

        let secp = MuSecp::new();
        let mut rng = rand::thread_rng();

        // Create two parties
        let mut sk1_bytes = [0u8; 32];
        let mut sk2_bytes = [0u8; 32];
        rng.fill(&mut sk1_bytes);
        rng.fill(&mut sk2_bytes);
        
        let sk1 = SecretKey::from_byte_array(sk1_bytes).expect("valid secret key");
        let sk2 = SecretKey::from_byte_array(sk2_bytes).expect("valid secret key");
        let kp1 = Keypair::from_secret_key(&secp, &sk1);
        let kp2 = Keypair::from_secret_key(&secp, &sk2);

        // Aggregate public keys
        let pubkeys = vec![MuPubKey::from(kp1), MuPubKey::from(kp2)];
        let key_agg = KeyAggContext::new(pubkeys).expect("key aggregation failed");
        let agg_pk: MuPubKey = key_agg.aggregated_pubkey();

        // Create message
        let msg = b"test message for musig2";

        // First round: generate nonces
        let spices1 = SecNonceSpices::new()
            .with_seckey(kp1.secret_key())
            .with_message(msg);
        let spices2 = SecNonceSpices::new()
            .with_seckey(kp2.secret_key())
            .with_message(msg);

        // Create nonce seeds
        let nonce_seed1 = [0u8; 32];
        let nonce_seed2 = [1u8; 32];

        let mut fr1 = FirstRound::new(key_agg.clone(), nonce_seed1, 0, spices1).unwrap();
        let mut fr2 = FirstRound::new(key_agg.clone(), nonce_seed2, 1, spices2).unwrap();

        // Exchange nonces
        let pubnonce1 = fr1.our_public_nonce();
        let pubnonce2 = fr2.our_public_nonce();

        fr1.receive_nonce(1, pubnonce2).unwrap();
        fr2.receive_nonce(0, pubnonce1).unwrap();

        // Second round: sign
        let r2_1 = fr1.finalize(kp1.secret_key(), msg).unwrap();
        let r2_2 = fr2.finalize(kp2.secret_key(), msg).unwrap();

        let partial2: Option<musig2::secp::Scalar> = r2_2.our_signature();

        // Finalize signatures
        let mut r2_1_final = r2_1;
        if let Some(p2) = partial2 {
            r2_1_final.receive_signature(1, p2).unwrap();
        }
        let sig: musig2::CompactSignature = r2_1_final.finalize().unwrap();

        // Verify the signature using musig2's verify_single function
        musig2::verify_single(agg_pk, sig, msg).expect("signature verification failed");
    }
}