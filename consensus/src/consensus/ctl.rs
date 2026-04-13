use super::{factory::MultiConsensusManagementStore, Consensus};
use parking_lot::RwLock;
use spora_consensusmanager::ConsensusCtl;
use std::{sync::Arc, thread::JoinHandle};

pub struct Ctl {
    management_store: Arc<RwLock<MultiConsensusManagementStore>>,
    consensus: Arc<Consensus>,
}

impl Ctl {
    pub fn new(management_store: Arc<RwLock<MultiConsensusManagementStore>>, consensus: Arc<Consensus>) -> Self {
        Self { management_store, consensus }
    }
}

impl ConsensusCtl for Ctl {
    fn start(&self) -> Vec<JoinHandle<()>> {
        self.consensus.run_processors()
    }

    fn stop(&self) {
        self.consensus.signal_exit()
    }

    fn make_active(&self) {
        // TODO: pass a value to make sure the correct consensus is committed
        self.management_store.write().commit_staging_consensus().unwrap();
    }
}

/// Impl for test purposes
impl ConsensusCtl for Consensus {
    fn start(&self) -> Vec<JoinHandle<()>> {
        self.run_processors()
    }

    fn stop(&self) {
        self.signal_exit()
    }

    fn make_active(&self) {
        // Fixed-consensus instances have no staging slot to commit into.
        // Treating this as a no-op keeps tests and embedded single-consensus setups safe.
    }
}
