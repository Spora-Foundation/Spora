//! MuSig2 wrapper for Copperoot using standard musig2 crate
//!
//! This module provides a simplified wrapper around the musig2 crate,
//! using standard BIP340 hashing (SHA256) instead of BLAKE3.

// Re-export types from musig2 crate
pub use musig2::{
    errors::{KeyAggError, SigningError, VerifyError},
    CompactSignature, FirstRound, KeyAggContext as MuSig2KeyAgg, NonceSeed, PartialSignature, PubNonce, SecNonceSpices, SecondRound,
};

use secp256k1::{All, Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use spora_hashes::Hash;
use std::collections::HashMap;

/// MuSig2 nonce wrapper (for compatibility)
#[derive(Debug, Clone)]
pub struct MuSig2Nonce {
    /// Public nonce
    pub public_nonce: PubNonce,
}

/// MuSig2 round 2 wrapper (for compatibility)
pub struct MuSig2Round2 {
    inner: SecondRound,
}

impl MuSig2Round2 {
    pub fn receive_signature(&mut self, peer_idx: usize, sig: musig2::secp::Scalar) -> Result<(), MuSig2Error> {
        self.inner.receive_signature(peer_idx, sig).map_err(|_| MuSig2Error::SigningFailed)
    }

    pub fn finalize(self) -> Result<CompactSignature, MuSig2Error> {
        self.inner.finalize().map_err(|_| MuSig2Error::SigningFailed)
    }
}

/// MuSig2 session wrapper (for compatibility)
pub struct MuSig2Session {
    key_agg: MuSig2KeyAgg,
    my_index: usize,
    first_round: FirstRound,
    nonces: HashMap<usize, MuSig2Nonce>,
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
    /// *Note*: The actual nonce generation happens in FirstRound::new, this is just a thin wrapper
    pub fn new(
        key_agg: &MuSig2KeyAgg,
        my_index: usize,
        seckey: &SecretKey,
        msg: &[u8],
        nonce_seed: NonceSeed,
    ) -> Result<Self, MuSig2Error> {
        let spices = SecNonceSpices::new().with_seckey(seckey).with_message(msg);
        let fr = FirstRound::new(key_agg.clone(), nonce_seed, my_index, spices).map_err(|_| MuSig2Error::NonceGenerationFailed)?;
        Ok(Self { public_nonce: fr.our_public_nonce() })
    }
}

impl MuSig2Session {
    /// Create a new MuSig2 session (simplified interface)
    /// Creates a session with FirstRound built-in and stores "our" PubNonce in nonces
    pub fn new(
        key_agg: MuSig2KeyAgg,
        my_index: usize,
        seckey: &SecretKey,
        msg: &[u8],
        nonce_seed: NonceSeed,
    ) -> Result<Self, MuSig2Error> {
        let spices = SecNonceSpices::new().with_seckey(seckey).with_message(msg);

        let first_round =
            FirstRound::new(key_agg.clone(), nonce_seed, my_index, spices).map_err(|_| MuSig2Error::SessionCreationFailed)?;

        let mut nonces = HashMap::new();
        nonces.insert(my_index, MuSig2Nonce { public_nonce: first_round.our_public_nonce() });

        Ok(Self {
            key_agg,
            my_index,
            first_round,
            nonces,
            session_id: Hash::default(), // Use random or derived value if real session ID needed
        })
    }

    /// Get our public nonce
    pub fn our_public_nonce(&self) -> PubNonce {
        self.first_round.our_public_nonce()
    }

    /// Receive a nonce from a peer
    pub fn receive_nonce(&mut self, peer_idx: usize, pubnonce: PubNonce) -> Result<(), MuSig2Error> {
        self.first_round.receive_nonce(peer_idx, pubnonce).map_err(|_| MuSig2Error::SessionCreationFailed)?;
        self.nonces.insert(peer_idx, MuSig2Nonce { public_nonce: pubnonce });
        Ok(())
    }

    /// Enter round 2, return round 2 handle + our partial signature (errors if peer nonces incomplete)
    pub fn finalize_round2(
        self,
        my_seckey: &SecretKey,
        msg: &[u8],
    ) -> Result<(MuSig2Round2, Option<musig2::secp::Scalar>), MuSig2Error> {
        let r2 = self.first_round.finalize(my_seckey, msg).map_err(|_| MuSig2Error::SigningFailed)?;
        let ours = r2.our_signature(); // Option<Scalar>
        Ok((MuSig2Round2 { inner: r2 }, ours))
    }

    /// Convenience method: when **all peer nonces and partial signatures** are collected,
    /// produce aggregated signature in one step
    ///
    /// `peers_partials`: list of (peer_idx, Scalar)
    pub fn sign(
        self,
        my_seckey: &SecretKey,
        msg: &[u8],
        peers_partials: &[(usize, musig2::secp::Scalar)],
    ) -> Result<MuSig2Signature, MuSig2Error> {
        let (mut round2, ours) = self.finalize_round2(my_seckey, msg)?;
        if let Some(our_sig) = ours {
            // Feed our partial signature back for unified flow (some implementations do this)
            // Note: some implementations don't require feeding back our own partial signature,
            // here we follow musig2 API convention and don't feed it back
            let _ = our_sig;
        }

        for (idx, sig) in peers_partials {
            round2.receive_signature(*idx, *sig)?;
        }
        let compact = round2.finalize()?;

        // Aggregated public key (for caller validation)
        let agg_pk = self.key_agg.aggregated_pubkey();
        let agg_x = agg_pk.x_only_public_key().0;

        Ok(MuSig2Signature { inner: compact, aggregated_key: XOnlyPublicKey::from_slice(&agg_x.serialize())? })
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
        use musig2::secp256k1::{Keypair, PublicKey as MuPubKey, Secp256k1 as MuSecp, SecretKey};
        use musig2::KeyAggContext;
        use rand::Rng;
        use secp256k1::rand;

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
    fn test_musig2_wrapper_two_party_sign() {
        use musig2::secp256k1::{Keypair, PublicKey as MuPubKey, Secp256k1 as MuSecp, SecretKey};
        use musig2::{KeyAggContext, NonceSeed};
        use rand::Rng;
        use secp256k1::rand;

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

        // Create message
        let msg = b"test message for musig2 wrapper";

        // Create sessions (FirstRound built-in)
        let mut s1 = MuSig2Session::new(key_agg.clone(), 0, &sk1, msg, NonceSeed([0; 32])).expect("session creation failed");
        let mut s2 = MuSig2Session::new(key_agg.clone(), 1, &sk2, msg, NonceSeed([1; 32])).expect("session creation failed");

        // Exchange nonces
        s1.receive_nonce(1, s2.our_public_nonce()).expect("nonce exchange failed");
        s2.receive_nonce(0, s1.our_public_nonce()).expect("nonce exchange failed");

        // Enter round 2, get partial signatures
        let (mut r2_1, part1) = s1.finalize_round2(&sk1, msg).expect("round 2 failed");
        let (mut r2_2, part2) = s2.finalize_round2(&sk2, msg).expect("round 2 failed");

        // Exchange partial signatures
        if let Some(p2) = part2 {
            r2_1.receive_signature(1, p2).expect("signature exchange failed");
        }
        if let Some(p1) = part1 {
            r2_2.receive_signature(0, p1).expect("signature exchange failed");
        }

        // Get final aggregated signature
        let sig = r2_1.finalize().expect("finalization failed");

        // Verify the signature using musig2's verify_single function
        let agg_pk = key_agg.aggregated_pubkey();
        musig2::verify_single(agg_pk, sig, msg).expect("signature verification failed");
    }

    #[test]
    fn test_musig2_two_party_sign() {
        use musig2::secp256k1::{Keypair, PublicKey as MuPubKey, Secp256k1 as MuSecp, SecretKey};
        use musig2::{FirstRound, KeyAggContext, SecNonceSpices};
        use rand::Rng;
        use secp256k1::rand;

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
        let spices1 = SecNonceSpices::new().with_seckey(kp1.secret_key()).with_message(msg);
        let spices2 = SecNonceSpices::new().with_seckey(kp2.secret_key()).with_message(msg);

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
