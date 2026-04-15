use spora_core::{
    sporad_env::{name, version},
    time::unix_now,
};
use spora_utils::networking::{NetAddress, PeerId};

/// Maximum allowed length for the user agent field in a version message `VersionMessage`.
pub const MAX_USER_AGENT_LEN: usize = 256;
/// Advertise the baseline full-node service bit until finer-grained service flags are introduced.
pub const DEFAULT_P2P_SERVICES: u64 = 1 << 0;

pub struct Version {
    pub protocol_version: u32,
    pub network: String,
    pub services: u64,
    pub timestamp: u64,
    pub address: Option<NetAddress>,
    pub id: PeerId,
    pub user_agent: String,
    pub disable_relay_tx: bool,
}

impl Version {
    pub fn new(address: Option<NetAddress>, id: PeerId, network: String, protocol_version: u32) -> Self {
        Self {
            protocol_version,
            network,
            services: DEFAULT_P2P_SERVICES,
            timestamp: unix_now(),
            address,
            id,
            user_agent: format!("/{}:{}/", name(), version()),
            disable_relay_tx: false,
        }
    }

    pub fn add_user_agent(&mut self, name: &str, version: &str, comments: &[String]) {
        let comments = if !comments.is_empty() { format!("({})", comments.join("; ")) } else { "".to_string() };
        let new_user_agent = format!("{}:{}{}", name, version, comments);
        self.user_agent = format!("{}{}/", self.user_agent, new_user_agent);
        self.user_agent.truncate(MAX_USER_AGENT_LEN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_new_advertises_default_services() {
        let version = Version::new(None, PeerId::default(), "simnet".to_string(), 7);
        assert_eq!(version.services, DEFAULT_P2P_SERVICES);
    }
}
