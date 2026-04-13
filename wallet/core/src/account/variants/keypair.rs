//!
//! Secp256k1 keypair account implementation
//!

use crate::account::Inner;
use crate::imports::*;
use secp256k1::PublicKey;
use spora_addresses::Version;

pub const KEYPAIR_ACCOUNT_KIND: &str = "spora-keypair-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "Keypair".to_string()
    }

    fn description(&self) -> String {
        "Secp265k1 Keypair Account".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(Keypair::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload {
    pub public_key: secp256k1::PublicKey,
    pub ecdsa: bool,
}

impl Payload {
    pub fn new(public_key: secp256k1::PublicKey, ecdsa: bool) -> Self {
        Self { public_key, ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    const STORAGE_MAGIC: u32 = 0x52494150;
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let public_key = self.public_key.serialize();

        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;

        BorshSerialize::serialize(public_key.as_slice(), writer)?;
        BorshSerialize::serialize(&self.ecdsa, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize_reader(reader)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let public_key_bytes: Vec<u8> = BorshDeserialize::deserialize_reader(reader)?;
        let public_key = secp256k1::PublicKey::from_slice(public_key_bytes.as_slice())
            .map_err(|_| IoError::new(IoErrorKind::Other, "Unable to deserialize keypair account (invalid public key)"))?;
        let ecdsa = BorshDeserialize::deserialize_reader(reader)?;

        Ok(Self { public_key, ecdsa })
    }
}

pub struct Keypair {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    public_key: PublicKey,
    ecdsa: bool,
}

impl Keypair {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        public_key: secp256k1::PublicKey,
        prv_key_data_id: PrvKeyDataId,
        ecdsa: bool,
    ) -> Result<Self> {
        let storable = Payload::new(public_key, ecdsa);
        let settings = AccountSettings { name, ..Default::default() };

        let (id, storage_key) = make_account_hashes(from_keypair(&prv_key_data_id, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let Payload { public_key, ecdsa, .. } = storable;
        Ok(Self { inner, prv_key_data_id, public_key, ecdsa })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, _meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Payload::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Payload { public_key, ecdsa, .. } = storable;
        Ok(Self { inner, prv_key_data_id: storage.prv_key_data_ids.clone().try_into()?, public_key, ecdsa })
    }
}

#[async_trait]
impl Account for Keypair {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        KEYPAIR_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn minimum_signatures(&self) -> u16 {
        1
    }

    fn receive_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize())?)
    }

    fn change_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize())?)
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();
        let storable = Payload::new(self.public_key, self.ecdsa);
        let account_storage = AccountStorage::try_new(
            KEYPAIR_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            self.prv_key_data_id.into(),
            settings,
            storable,
        )?;

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        Ok(None)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let addresses = self.account_addresses().ok();
        let descriptor = AccountDescriptor::new(
            KEYPAIR_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.balance(),
            self.prv_key_data_id.into(),
            self.receive_address().ok(),
            self.change_address().ok(),
            addresses,
        )
        .with_property(AccountDescriptorProperty::Ecdsa, self.ecdsa.into());

        Ok(descriptor)
    }

    fn create_address_private_keys<'l>(
        self: Arc<Self>,
        key_data: &PrvKeyData,
        payment_secret: &Option<Secret>,
        addresses: &[&'l Address],
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        let private_key =
            key_data.as_secret_key(payment_secret.as_ref())?.ok_or(Error::Custom("Unable to derive private key".to_string()))?;
        let mut private_keys = vec![];
        let wallet_address = self.receive_address()?;
        for address in addresses.iter() {
            if **address == wallet_address {
                private_keys.push(((*address).clone(), private_key));
            }
        }
        Ok(private_keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::make_xpub;
    use crate::wallet::Wallet;
    use spora_consensus_core::network::{NetworkId, NetworkType};

    #[tokio::test]
    async fn keypair_descriptor_exposes_single_address() -> Result<()> {
        let wallet = Arc::new(
            Wallet::try_with_rpc(None, Wallet::resident_store()?, None)?.with_network_id(NetworkId::new(NetworkType::Mainnet))?,
        );
        let account = Keypair::try_new(&wallet, None, *make_xpub().public_key(), PrvKeyDataId::new(0x1234_5678), false).await?;

        let receive = account.receive_address()?;
        let change = account.change_address()?;
        assert_eq!(receive, change);

        let addresses = account.account_addresses()?;
        assert_eq!(addresses, vec![receive.clone()]);

        let descriptor = account.descriptor()?;
        assert_eq!(descriptor.addresses, Some(vec![receive]));

        Ok(())
    }
}
