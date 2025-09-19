//!
//! Wallet data encryption module.
//!

use crate::imports::*;
use crate::result::Result;
use argon2::Argon2;
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    Key, XChaCha20Poly1305,
};
use std::ops::{Deref, DerefMut};
use zeroize::Zeroize;

/// Encryption algorithms supported by the Wallet framework.
#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EncryptionKind {
    #[default]
    XChaCha20Poly1305,
}

/// Abstract data container that can contain either plain or encrypted data and
/// transform the data between the two states.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "encryptable", content = "payload")]
pub enum Encryptable<T> {
    #[serde(rename = "plain")]
    Plain(T),
    #[serde(rename = "xchacha20poly1305")]
    XChaCha20Poly1305(Encrypted),
}

impl<T> Zeroize for Encryptable<T>
where
    T: Zeroize,
{
    fn zeroize(&mut self) {
        match self {
            Self::Plain(t) => t.zeroize(),
            Self::XChaCha20Poly1305(e) => e.zeroize(),
        }
    }
}

impl<T> Encryptable<T>
where
    T: Clone + Zeroize + BorshDeserialize + BorshSerialize,
{
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, Self::Plain(_))
    }

    pub fn decrypt(&self, secret: Option<&Secret>) -> Result<Decrypted<T>> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone())),
            Self::XChaCha20Poly1305(v) => {
                if let Some(secret) = secret {
                    Ok(v.decrypt(secret)?)
                } else {
                    Err("Decryption secret is 'None' when the data is encrypted!".into())
                }
            }
        }
    }

    pub fn encrypt(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Encrypted> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone()).encrypt(secret, encryption_kind)?),
            Self::XChaCha20Poly1305(v) => match encryption_kind {
                EncryptionKind::XChaCha20Poly1305 => Ok(v.clone()),
            },
        }
    }

    pub fn into_encrypted(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::XChaCha20Poly1305(Decrypted::new(v.clone()).encrypt(secret, encryption_kind)?)),
            Self::XChaCha20Poly1305(v) => Ok(Self::XChaCha20Poly1305(v.clone())),
        }
    }

    pub fn into_decrypted(self, secret: &Secret) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::Plain(v)),
            Self::XChaCha20Poly1305(v) => Ok(Self::Plain(v.decrypt::<T>(secret)?.unwrap())),
        }
    }
}

impl<T> From<T> for Encryptable<T> {
    fn from(value: T) -> Self {
        Encryptable::Plain(value)
    }
}

/// Abstract decrypted data container.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Decrypted<T>(pub(crate) T)
where
    T: BorshSerialize + BorshDeserialize;

impl<T> AsRef<T> for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsMut<T> for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn encrypt(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Encrypted> {
        let bytes = borsh::to_vec(&self.0)?;
        let encrypted = match encryption_kind {
            EncryptionKind::XChaCha20Poly1305 => encrypt_xchacha20poly1305(bytes.as_slice(), secret)?,
        };
        Ok(Encrypted::new(encryption_kind, encrypted))
    }

    pub fn unwrap(self) -> T {
        self.0
    }
}

/// Encrypted data container (wraps an encrypted payload)
#[derive(Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Encrypted {
    encryption_kind: EncryptionKind,
    payload: Vec<u8>,
}

impl Zeroize for Encrypted {
    fn zeroize(&mut self) {
        self.payload.zeroize();
    }
}

impl std::fmt::Debug for Encrypted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encrypted")
            .field("encryption_kind", &self.encryption_kind)
            .field("payload", &"[REDACTED]") // Don't expose encrypted payload in debug output
            .finish()
    }
}

impl Encrypted {
    pub fn new(encryption_kind: EncryptionKind, payload: Vec<u8>) -> Self {
        Encrypted { encryption_kind, payload }
    }

    pub fn replace(&mut self, from: Encrypted) {
        self.payload = from.payload;
    }

    pub fn kind(&self) -> EncryptionKind {
        self.encryption_kind
    }

    pub fn decrypt<T>(&self, secret: &Secret) -> Result<Decrypted<T>>
    where
        T: BorshSerialize + BorshDeserialize,
    {
        match self.encryption_kind {
            EncryptionKind::XChaCha20Poly1305 => {
                let decrypted = decrypt_xchacha20poly1305(&self.payload, secret)?;
                Ok(Decrypted(T::try_from_slice(decrypted.as_ref())?))
            }
        }
    }
}

/// Produces `BLAKE3` hash of the given data.
#[inline]
pub fn blake3_hash(data: &[u8]) -> Secret {
    let result = blake3::hash(data);
    Secret::new(result.as_bytes().to_vec())
}

/// Produces `BLAKE3d` hash of the given data (double hash).
/// This function is only used in WASM bindings and should not be used elsewhere.
#[inline]
pub fn blake3d_hash(data: &[u8]) -> Secret {
    let first = blake3_hash(data);
    blake3_hash(first.as_ref())
}

/// Produces `argon2blake3iv` hash of the given data.
pub fn argon2_blake3iv_hash(data: &[u8], byte_length: usize) -> Result<Secret> {
    let salt = blake3_hash(data); // Use BLAKE3 hash as salt
    let mut key = vec![0u8; byte_length];
    Argon2::default().hash_password_into(data, salt.as_ref(), &mut key)?;
    Ok(key.into())
}

/// Encrypts the given data using `XChaCha20Poly1305` algorithm with BLAKE3.
pub fn encrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Vec<u8>> {
    let private_key_bytes = argon2_blake3iv_hash(secret.as_ref(), 32)?;
    let key = Key::from_slice(private_key_bytes.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng); // 96-bits; unique per message
    let mut buffer = data.to_vec();
    buffer.reserve(16); // Reserve space for authentication tag
    cipher.encrypt_in_place(&nonce, &[], &mut buffer).map_err(|e| Error::custom(format!("Encryption failed: {}", e)))?;
    buffer.splice(0..0, nonce.iter().cloned());
    Ok(buffer)
}

/// Decrypts the given data using `XChaCha20Poly1305` algorithm with BLAKE3.
pub fn decrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Secret> {
    if data.len() < 24 {
        return Err(Error::custom("Encrypted data too short"));
    }

    let private_key_bytes = argon2_blake3iv_hash(secret.as_ref(), 32)?;
    let key = Key::from_slice(private_key_bytes.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = &data[0..24];
    let mut buffer = data[24..].to_vec();
    cipher.decrypt_in_place(nonce.into(), &[], &mut buffer).map_err(|e| Error::custom(format!("Decryption failed: {}", e)))?;
    Ok(Secret::new(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_argon2() {
        println!("testing argon2 hash");
        let password = b"user_password";
        let hash = argon2_blake3iv_hash(password, 32).unwrap();
        let hash_hex = hash.as_ref().to_hex();
        // println!("argon2hash: {:?}", hash_hex);
        assert_eq!(hash_hex, "7417f7d86f98257d447be45d151869f3110daf951ef0b95ef7b9e11a0f03c492");
    }

    #[test]
    fn test_wallet_encrypt_decrypt() -> Result<()> {
        println!("testing encrypt/decrypt");

        let password = b"password";
        let original = b"hello world".to_vec();
        // println!("original: {}", original.to_hex());
        let password = Secret::new(password.to_vec());
        let encrypted = encrypt_xchacha20poly1305(&original, &password).unwrap();
        // println!("encrypted: {}", encrypted.to_hex());
        let decrypted = decrypt_xchacha20poly1305(&encrypted, &password).unwrap();
        // println!("decrypted: {}", decrypted.to_hex());
        assert_eq!(decrypted.as_ref(), original);

        Ok(())
    }
}
