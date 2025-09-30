//! Native MuSig2 implementation for Copperoot
//!
//! This module implements MuSig2 key aggregation and signing using BLAKE3-256
//! for all hash operations, maintaining compatibility with BIP340 Schnorr signatures.

use blake3::Hasher;
use secp256k1::{Keypair, Message, PublicKey, Secp256k1, SecretKey, XOnlyPublicKey};
use std::collections::HashMap;
use tondi_hashes::Hash;

/// MuSig2 key aggregation context
#[derive(Debug, Clone)]
pub struct MuSig2KeyAgg {
    /// Aggregated public key
    pub aggregated_key: XOnlyPublicKey,
    /// Key aggregation coefficient
    pub key_coefficient: SecretKey,
    /// Public key list hash
    pub key_list_hash: Hash,
}

/// MuSig2 nonce
#[derive(Debug, Clone)]
pub struct MuSig2Nonce {
    /// Public nonce
    pub public_nonce: PublicKey,
    /// Secret nonce
    pub secret_nonce: SecretKey,
}

/// MuSig2 session
#[derive(Debug, Clone)]
pub struct MuSig2Session {
    /// Key aggregation context
    pub key_agg: MuSig2KeyAgg,
    /// Nonces from all participants
    pub nonces: HashMap<usize, MuSig2Nonce>,
    /// Aggregated nonce
    pub aggregated_nonce: PublicKey,
    /// Session ID
    pub session_id: Hash,
}

/// MuSig2 signature
#[derive(Debug, Clone)]
pub struct MuSig2Signature {
    /// Final signature
    pub signature: secp256k1::schnorr::Signature,
    /// Aggregated public key
    pub aggregated_key: XOnlyPublicKey,
}

impl MuSig2KeyAgg {
    /// Create a new key aggregation context
    pub fn new(public_keys: &[XOnlyPublicKey]) -> Result<Self, MuSig2Error> {
        if public_keys.is_empty() {
            return Err(MuSig2Error::EmptyKeyList);
        }

        let secp = Secp256k1::new();
        
        // Compute key list hash
        let key_list_hash = Self::compute_key_list_hash(public_keys);
        
        // Compute key aggregation coefficient
        let key_coefficient = Self::compute_key_coefficient(&key_list_hash, &public_keys[0]);
        
        // Compute aggregated public key
        let mut aggregated_key = public_keys[0].public_key(&secp);
        aggregated_key = aggregated_key.mul_tweak(&secp, &key_coefficient.into())?;
        
        for (i, pubkey) in public_keys.iter().enumerate().skip(1) {
            let coeff = Self::compute_key_coefficient(&key_list_hash, pubkey);
            let tweaked_pubkey = pubkey.public_key(&secp).mul_tweak(&secp, &coeff.into())?;
            aggregated_key = aggregated_key.combine(&tweaked_pubkey)?;
        }
        
        Ok(Self {
            aggregated_key: XOnlyPublicKey::from(aggregated_key),
            key_coefficient,
            key_list_hash,
        })
    }

    /// Compute key list hash
    fn compute_key_list_hash(public_keys: &[XOnlyPublicKey]) -> Hash {
        let mut hasher = Hasher::new();
        hasher.update(b"MuSig2KeyAggList");
        for pubkey in public_keys {
            hasher.update(pubkey.serialize());
        }
        Hash::from_slice(hasher.finalize().as_bytes())
    }

    /// Compute key aggregation coefficient
    fn compute_key_coefficient(key_list_hash: &Hash, pubkey: &XOnlyPublicKey) -> SecretKey {
        let mut hasher = Hasher::new();
        hasher.update(b"MuSig2KeyAggCoeff");
        hasher.update(key_list_hash.as_bytes());
        hasher.update(pubkey.serialize());
        let hash = hasher.finalize();
        SecretKey::from_slice(hash.as_bytes()).unwrap()
    }
}

impl MuSig2Nonce {
    /// Generate a new nonce
    pub fn new(secp: &Secp256k1, keypair: &Keypair, message: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(b"MuSig2Nonce");
        hasher.update(keypair.secret_key().as_ref());
        hasher.update(message);
        let hash = hasher.finalize();
        
        let secret_nonce = SecretKey::from_slice(hash.as_bytes()).unwrap();
        let public_nonce = PublicKey::from_secret_key(secp, &secret_nonce);
        
        Self {
            public_nonce,
            secret_nonce,
        }
    }
}

impl MuSig2Session {
    /// Create a new MuSig2 session
    pub fn new(
        key_agg: MuSig2KeyAgg,
        nonces: HashMap<usize, MuSig2Nonce>,
    ) -> Result<Self, MuSig2Error> {
        if nonces.is_empty() {
            return Err(MuSig2Error::EmptyNonceList);
        }

        let secp = Secp256k1::new();
        
        // Aggregate nonces
        let mut aggregated_nonce = nonces.values().next().unwrap().public_nonce;
        for nonce in nonces.values().skip(1) {
            aggregated_nonce = aggregated_nonce.combine(&nonce.public_nonce)?;
        }
        
        // Generate session ID
        let mut hasher = Hasher::new();
        hasher.update(b"MuSig2Session");
        hasher.update(key_agg.aggregated_key.serialize());
        hasher.update(aggregated_nonce.serialize());
        let session_id = Hash::from_slice(hasher.finalize().as_bytes());
        
        Ok(Self {
            key_agg,
            nonces,
            aggregated_nonce,
            session_id,
        })
    }

    /// Sign a message
    pub fn sign(
        &self,
        secp: &Secp256k1,
        keypair: &Keypair,
        message: &[u8],
        participant_index: usize,
    ) -> Result<MuSig2Signature, MuSig2Error> {
        let nonce = self.nonces.get(&participant_index)
            .ok_or(MuSig2Error::InvalidParticipantIndex)?;
        
        // Compute challenge
        let challenge = self.compute_challenge(message)?;
        
        // Compute signature
        let signature = self.compute_signature(secp, keypair, &challenge, nonce)?;
        
        Ok(MuSig2Signature {
            signature,
            aggregated_key: self.key_agg.aggregated_key,
        })
    }

    /// Compute challenge hash
    fn compute_challenge(&self, message: &[u8]) -> Result<SecretKey, MuSig2Error> {
        let mut hasher = Hasher::new();
        hasher.update(b"MuSig2Challenge");
        hasher.update(self.key_agg.aggregated_key.serialize());
        hasher.update(self.aggregated_nonce.serialize());
        hasher.update(message);
        let hash = hasher.finalize();
        Ok(SecretKey::from_slice(hash.as_bytes()).unwrap())
    }

    /// Compute signature
    fn compute_signature(
        &self,
        secp: &Secp256k1,
        keypair: &Keypair,
        challenge: &SecretKey,
        nonce: &MuSig2Nonce,
    ) -> Result<secp256k1::schnorr::Signature, MuSig2Error> {
        // Compute key coefficient
        let key_coeff = MuSig2KeyAgg::compute_key_coefficient(
            &self.key_agg.key_list_hash,
            &keypair.x_only_public_key().0,
        );
        
        // Compute signature
        let mut signature = nonce.secret_nonce;
        let tweaked_secret = keypair.secret_key().mul_tweak(&key_coeff.into())?;
        let challenge_times_secret = challenge.mul_tweak(&tweaked_secret.into())?;
        signature = signature.add_tweak(&challenge_times_secret.into())?;
        
        // Create Schnorr signature
        let msg = Message::from_slice(signature.as_ref())?;
        Ok(secp256k1::schnorr::Signature::from_slice(&signature.as_ref())?)
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
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
    #[error("Invalid signature")]
    InvalidSignature,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_musig2_key_aggregation() {
        let secp = Secp256k1::new();
        let keypair1 = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let keypair2 = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        
        let pubkeys = vec![
            keypair1.x_only_public_key().0,
            keypair2.x_only_public_key().0,
        ];
        
        let key_agg = MuSig2KeyAgg::new(&pubkeys).unwrap();
        assert_eq!(key_agg.aggregated_key.serialize().len(), 32);
    }

    #[test]
    fn test_musig2_nonce_generation() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let message = b"test message";
        
        let nonce = MuSig2Nonce::new(&secp, &keypair, message);
        assert_eq!(nonce.public_nonce.serialize().len(), 33);
    }

    #[test]
    fn test_musig2_session() {
        let secp = Secp256k1::new();
        let keypair1 = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        let keypair2 = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
        
        let pubkeys = vec![
            keypair1.x_only_public_key().0,
            keypair2.x_only_public_key().0,
        ];
        
        let key_agg = MuSig2KeyAgg::new(&pubkeys).unwrap();
        
        let mut nonces = HashMap::new();
        let message = b"test message";
        nonces.insert(0, MuSig2Nonce::new(&secp, &keypair1, message));
        nonces.insert(1, MuSig2Nonce::new(&secp, &keypair2, message));
        
        let session = MuSig2Session::new(key_agg, nonces).unwrap();
        assert_eq!(session.session_id.as_bytes().len(), 32);
    }
}
