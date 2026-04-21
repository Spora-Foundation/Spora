use crate::tasks::daemon::DaemonArgs;
#[cfg(feature = "devnet-prealloc")]
use spora_addresses::Address;
use sporad_lib::args::Args;

pub struct ArgsBuilder {
    args: Args,
}

impl ArgsBuilder {
    #[cfg(feature = "devnet-prealloc")]
    pub fn devnet(num_prealloc_cells: u64, prealloc_amount: u64) -> Self {
        let args = Args {
            devnet: true,
            disable_upnp: true,
            enable_unsynced_mining: true,
            skip_proof_of_work: true,
            num_prealloc_cells: Some(num_prealloc_cells),
            prealloc_amount: prealloc_amount * spora_consensus_core::constants::SAU_PER_SPORA,
            block_template_cache_lifetime: Some(0),
            rpc_max_clients: 2500,
            unsafe_rpc: true,
            ..Default::default()
        };

        Self { args }
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn simnet(num_prealloc_cells: u64, prealloc_amount: u64) -> Self {
        let args = Args {
            simnet: true,
            disable_upnp: true, // UPnP registration might take some time and is not needed for this test
            enable_unsynced_mining: true,
            num_prealloc_cells: Some(num_prealloc_cells),
            prealloc_amount: prealloc_amount * spora_consensus_core::constants::SAU_PER_SPORA,
            block_template_cache_lifetime: Some(0),
            rpc_max_clients: 2500,
            unsafe_rpc: true,
            ..Default::default()
        };

        Self { args }
    }

    #[cfg(not(feature = "devnet-prealloc"))]
    pub fn simnet() -> Self {
        let args = Args {
            simnet: true,
            disable_upnp: true, // UPnP registration might take some time and is not needed for this test
            enable_unsynced_mining: true,
            block_template_cache_lifetime: Some(0),
            rpc_max_clients: 2500,
            unsafe_rpc: true,
            ..Default::default()
        };

        Self { args }
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn prealloc_address(mut self, prealloc_address: Address) -> Self {
        self.args.prealloc_address = Some(prealloc_address.to_string());
        self
    }

    pub fn rpc_max_clients(mut self, rpc_max_clients: usize) -> Self {
        self.args.rpc_max_clients = rpc_max_clients;
        self
    }

    pub fn max_tracked_addresses(mut self, max_tracked_addresses: usize) -> Self {
        self.args.max_tracked_addresses = max_tracked_addresses;
        self
    }

    pub fn cellindex(mut self, cellindex: bool) -> Self {
        self.args.cellindex = cellindex;
        self
    }

    pub fn relay_non_standard(mut self, relay_non_standard: bool) -> Self {
        self.args.relay_non_std = relay_non_standard;
        self.args.reject_non_std = false;
        self
    }

    pub fn block_max_mass(mut self, block_max_mass: u64) -> Self {
        self.args.block_max_mass = Some(block_max_mass);
        self
    }

    pub fn resumable_virtual_state_step_cycles(mut self, step_cycles: u64) -> Self {
        self.args.resumable_virtual_state_step_cycles = Some(step_cycles);
        self
    }

    pub fn apply_args<F>(mut self, edit_func: F) -> Self
    where
        F: Fn(&mut Args),
    {
        edit_func(&mut self.args);
        self
    }

    pub fn apply_daemon_args(mut self, daemon_args: &DaemonArgs) -> Self {
        daemon_args.apply_to(&mut self.args);
        self
    }

    pub fn build(self) -> Args {
        self.args
    }
}
