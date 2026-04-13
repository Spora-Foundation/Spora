use crate::imports::*;
use chacha20poly1305::{aead::Aead, KeyInit, XChaCha20Poly1305};

#[derive(Debug, Clone)]
pub struct EncryptedMnemonic<T: AsRef<[u8]>> {
    pub cipher: T,
    pub salt: T,
}

#[derive(Debug, Clone)]
pub struct SingleWalletFileV1<T: AsRef<[u8]>> {
    pub encrypted_mnemonic: EncryptedMnemonic<T>,
    pub xpublic_key: String,
    pub ecdsa: bool,
}

impl<T: AsRef<[u8]>> SingleWalletFileV1<T> {
    pub const NUM_THREADS: u32 = 8;
}

#[derive(Debug, Clone)]
pub struct MultisigWalletFileV1<T: AsRef<[u8]>> {
    pub encrypted_mnemonics: Vec<EncryptedMnemonic<T>>,
    pub xpublic_keys: Vec<String>,
    pub required_signatures: u16,
    pub cosigner_index: u8,
    pub ecdsa: bool,
}

impl<T: AsRef<[u8]>> MultisigWalletFileV1<T> {
    pub const NUM_THREADS: u32 = 8;
}

#[derive(Debug, Clone)]
pub enum Gen1WalletFile {
    SingleV1(SingleWalletFileV1<Vec<u8>>),
    MultiV1(MultisigWalletFileV1<Vec<u8>>),
}

pub fn decrypt_mnemonic<T: AsRef<[u8]>>(
    num_threads: u32,
    EncryptedMnemonic { cipher, salt }: EncryptedMnemonic<T>,
    pass: &[u8],
) -> Result<String> {
    let params = argon2::ParamsBuilder::new().t_cost(1).m_cost(64 * 1024).p_cost(num_threads).output_len(32).build().unwrap();
    let mut key = [0u8; 32];
    argon2::Argon2::new(argon2::Algorithm::Argon2id, Default::default(), params)
        .hash_password_into(pass, salt.as_ref(), &mut key[..])
        .map_err(|err| Error::custom(format!("unable to derive gen1 import key: {err}")))?;

    let aead =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|err| Error::custom(format!("unable to initialize gen1 cipher: {err}")))?;
    let (nonce, ciphertext) = cipher.as_ref().split_at(24);
    let decrypted =
        aead.decrypt(nonce.into(), ciphertext).map_err(|err| Error::custom(format!("unable to decrypt gen1 mnemonic: {err}")))?;

    String::from_utf8(decrypted).map_err(|_| Error::custom("gen1 mnemonic payload is not valid utf-8"))
}

#[derive(Debug, Default, Deserialize)]
struct EncryptedMnemonicIntermediate {
    #[serde(with = "spora_utils::serde_bytes")]
    cipher: Vec<u8>,
    #[serde(with = "spora_utils::serde_bytes")]
    salt: Vec<u8>,
}

impl From<EncryptedMnemonicIntermediate> for EncryptedMnemonic<Vec<u8>> {
    fn from(value: EncryptedMnemonicIntermediate) -> Self {
        Self { cipher: value.cipher, salt: value.salt }
    }
}

#[derive(serde_repr::Deserialize_repr, PartialEq, Debug)]
#[repr(u8)]
enum WalletVersion {
    One = 1,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnifiedWalletIntermediate {
    version: WalletVersion,
    encrypted_mnemonics: Vec<EncryptedMnemonicIntermediate>,
    public_keys: Vec<String>,
    minimum_signatures: u16,
    cosigner_index: u8,
    ecdsa: bool,
}

impl UnifiedWalletIntermediate {
    fn into_wallet_type(mut self) -> Result<Gen1WalletFile> {
        if self.version != WalletVersion::One {
            return Err(Error::custom("unsupported gen1 wallet version"));
        }

        let single = self.encrypted_mnemonics.len() == 1 && self.public_keys.len() == 1;
        if self.encrypted_mnemonics.is_empty() || self.public_keys.is_empty() {
            return Err(Error::custom("gen1 wallet data does not contain keys"));
        }

        if single {
            Ok(Gen1WalletFile::SingleV1(SingleWalletFileV1 {
                encrypted_mnemonic: self.encrypted_mnemonics.remove(0).into(),
                xpublic_key: self.public_keys.remove(0),
                ecdsa: self.ecdsa,
            }))
        } else {
            Ok(Gen1WalletFile::MultiV1(MultisigWalletFileV1 {
                encrypted_mnemonics: self.encrypted_mnemonics.into_iter().map(EncryptedMnemonic::from).collect(),
                xpublic_keys: self.public_keys,
                required_signatures: self.minimum_signatures,
                cosigner_index: self.cosigner_index,
                ecdsa: self.ecdsa,
            }))
        }
    }
}

pub fn parse_gen1_wallet_file(wallet_data: &[u8]) -> Result<Gen1WalletFile> {
    let unified: UnifiedWalletIntermediate =
        serde_json::from_slice(wallet_data).map_err(|err| Error::custom(format!("invalid gen1 wallet json: {err}")))?;
    unified.into_wallet_type()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gen1_wallet_file_supports_single_v1() {
        let single_json_v1 = r#"{"version":1,"encryptedMnemonics":[{"cipher":"2022041df1a5bdcc26445952c53f96518641118bf0f990a01747d631d4607e5b53af3c9f4c07d6e3b84bc766445191b13d1f1fdf7ac96eae9c8859a9add660ac15b938356f936fdf614640d89627d368c57b22cf62844b1e1bcf3feceecbc6bf655df9519d7e3cfede6fe19d87a49e5709211b0b95c8d68781c70c4722bd8e25361492ef38d5cca21664a7f0838e4a1e2994d30c6d4b81d1397169570375ce56608439ae00e84c1f6acdd805f0ee22d4ba7b354c7f7cd4b2d18ce4fd6b8af785f95ed2a69361f318bc","salt":"044f5b890e48af4a7dcd7e7766af9380"}],"publicKeys":["kpub2KUE88roSn5peP1rEZnbRuKYw1fEPbhqBoXVWW7mLfkrLvQBAjUqwx7m1ezeSfqfecv9RUYePuHf99iW51i31WjwWjnzKDCUcTucBSiBbJA"],"minimumSignatures":1,"cosignerIndex":0,"lastUsedExternalIndex":0,"lastUsedInternalIndex":0,"ecdsa":false}"#;

        let parsed = parse_gen1_wallet_file(single_json_v1.as_bytes()).unwrap();
        match parsed {
            Gen1WalletFile::SingleV1(file) => {
                assert_eq!(
                    file.xpublic_key,
                    "kpub2KUE88roSn5peP1rEZnbRuKYw1fEPbhqBoXVWW7mLfkrLvQBAjUqwx7m1ezeSfqfecv9RUYePuHf99iW51i31WjwWjnzKDCUcTucBSiBbJA"
                );
                assert!(!file.ecdsa);
            }
            Gen1WalletFile::MultiV1(_) => panic!("expected single-v1 wallet data"),
        }
    }
}
