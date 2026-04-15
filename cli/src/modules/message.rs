use spora_addresses::Version;
use spora_bip32::secp256k1::{PublicKey, SecretKey, XOnlyPublicKey};
use spora_wallet_core::message::SignMessageOptions;
use spora_wallet_core::{
    account::{BIP32_ACCOUNT_KIND, KEYPAIR_ACCOUNT_KIND},
    message::{sign_message, verify_message, PersonalMessage},
};

use crate::imports::*;

#[derive(Default)]
pub struct Message;

#[async_trait]
impl Handler for Message {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("message")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Sign a message or verify a message signature"
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<SporaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Message {
    async fn main(self: Arc<Self>, ctx: Arc<SporaCli>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        match argv.first().unwrap().as_str() {
            "sign" => {
                if argv.len() != 2 {
                    return self.display_help(ctx, argv).await;
                }

                let spora_address = argv[1].as_str();
                let asked_message = ctx.term().ask(false, "Message: ").await?;
                let message = asked_message.as_str();

                self.sign(ctx, spora_address, message).await?;
            }
            "verify" => {
                if argv.len() != 3 {
                    return self.display_help(ctx, argv).await;
                }
                let spora_address = argv[1].as_str();
                let signature = argv[2].as_str();
                let asked_message = ctx.term().ask(false, "Message: ").await?;
                let message = asked_message.as_str();

                self.verify(ctx, spora_address, signature, message).await?;
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<SporaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("sign <spora_address>", "Sign a message with the private key that matches the given address. Prompts for message."),
                ("verify <spora_address> <signature>", "Verify a 96-byte StdSingle signature envelope (xonly-pubkey||signature)."),
            ],
            None,
        )?;

        Ok(())
    }

    async fn sign(self: Arc<Self>, ctx: Arc<SporaCli>, spora_address: &str, message: &str) -> Result<()> {
        let spora_address = Address::try_from(spora_address)?;
        if spora_address.version != Version::StdSingle {
            return Err(spora_addresses::AddressError::InvalidVersion(spora_address.version as u8).into());
        }

        let pm = PersonalMessage(message);
        let privkey = self.clone().get_address_private_key(&ctx, spora_address.clone()).await?;
        let sign_options = SignMessageOptions { no_aux_rand: false };

        let sig_result = sign_message(&pm, &privkey, &sign_options);

        match sig_result {
            Ok(signature) => {
                let secret_key = SecretKey::from_slice(&privkey).map_err(|e| Error::custom(e.to_string()))?;
                let public_key = PublicKey::from_secret_key_global(&secret_key);
                let xonly_pubkey = public_key.x_only_public_key().0.serialize();
                self.ensure_stdsingle_pubkey_matches_address(&spora_address, &xonly_pubkey)?;

                let mut envelope = Vec::with_capacity(96);
                envelope.extend_from_slice(&xonly_pubkey);
                envelope.extend_from_slice(signature.as_slice());
                let envelope_hex = faster_hex::hex_string(envelope.as_slice());
                tprintln!(ctx, "Signature: {}", envelope_hex);
                Ok(())
            }
            Err(_) => Err(Error::custom("Message signing failed")),
        }
    }

    async fn verify(self: Arc<Self>, ctx: Arc<SporaCli>, spora_address: &str, signature: &str, message: &str) -> Result<()> {
        let spora_address = Address::try_from(spora_address)?;
        if spora_address.version != Version::StdSingle {
            return Err(spora_addresses::AddressError::InvalidVersion(spora_address.version as u8).into());
        }

        let signature_hex = self.decode_signature_hex(signature)?;
        let (pubkey, signature) = match signature_hex.len() {
            96 => {
                let pubkey_slice: [u8; 32] =
                    signature_hex[0..32].try_into().map_err(|_| Error::custom("Invalid signature envelope pubkey"))?;
                let signature_slice: [u8; 64] =
                    signature_hex[32..96].try_into().map_err(|_| Error::custom("Invalid signature envelope payload"))?;
                self.ensure_stdsingle_pubkey_matches_address(&spora_address, &pubkey_slice)?;
                (XOnlyPublicKey::from_slice(&pubkey_slice).map_err(|e| Error::custom(e.to_string()))?, signature_slice.to_vec())
            }
            _ => {
                return Err(Error::custom("Invalid signature length; expected 192 hex chars (StdSingle envelope)"));
            }
        };

        let pm = PersonalMessage(message);
        let verify_result = verify_message(&pm, &signature, &pubkey);

        match verify_result {
            Ok(()) => {
                tprintln!(ctx, "Message verified successfully!");
            }
            Err(_) => {
                return Err(Error::custom("Verification failed"));
            }
        }

        Ok(())
    }

    fn decode_signature_hex(&self, signature: &str) -> Result<Vec<u8>> {
        if signature.len() % 2 != 0 {
            return Err(Error::custom("Invalid hex signature length"));
        }
        let mut decoded = vec![0u8; signature.len() / 2];
        faster_hex::hex_decode(signature.as_bytes(), &mut decoded)?;
        Ok(decoded)
    }

    fn ensure_stdsingle_pubkey_matches_address(&self, address: &Address, pubkey: &[u8; 32]) -> Result<()> {
        let expected_address = Address::new_std_single(address.prefix, pubkey)?;
        if expected_address.payload != address.payload {
            return Err(Error::custom("Signature pubkey does not match the provided StdSingle address"));
        }
        Ok(())
    }

    async fn get_address_private_key(self: Arc<Self>, ctx: &Arc<SporaCli>, spora_address: Address) -> Result<[u8; 32]> {
        let account = ctx.wallet().account()?;

        match account.account_kind().as_ref() {
            BIP32_ACCOUNT_KIND => {
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;
                let keydata = account.prv_key_data(wallet_secret).await?;
                let account = account.clone().as_derivation_capable().expect("expecting derivation capable");

                let (receive, change) = account.derivation().addresses_indexes(&[&spora_address])?;
                let private_keys = account.create_private_keys(&keydata, &payment_secret, &receive, &change)?;
                for (address, private_key) in private_keys {
                    if spora_address == *address {
                        return Ok(private_key.secret_bytes());
                    }
                }

                Err(spora_wallet_core::error::Error::AddressNotFound.into())
            }
            KEYPAIR_ACCOUNT_KIND => {
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;
                let keydata = account.prv_key_data(wallet_secret).await?;
                let decrypted_privkey = keydata.payload.decrypt(payment_secret.as_ref()).unwrap();
                let secretkey = decrypted_privkey.as_secret_key()?.unwrap();
                Ok(secretkey.secret_bytes())
            }
            _ => Err(Error::custom("Unsupported account kind")),
        }
    }
}
