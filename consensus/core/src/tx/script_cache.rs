use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct ScriptCacheCounters {
    pub insert_counts: AtomicU64,
    pub get_counts: AtomicU64,
}

impl ScriptCacheCounters {
    pub fn snapshot(&self) -> ScriptCacheCountersSnapshot {
        ScriptCacheCountersSnapshot {
            insert_counts: self.insert_counts.load(Ordering::Relaxed),
            get_counts: self.get_counts.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScriptCacheCountersSnapshot {
    pub insert_counts: u64,
    pub get_counts: u64,
}

impl ScriptCacheCountersSnapshot {
    pub fn hit_ratio(&self) -> f64 {
        if self.insert_counts > 0 {
            self.get_counts as f64 / self.insert_counts as f64
        } else {
            0.0
        }
    }
}

impl core::ops::Sub for &ScriptCacheCountersSnapshot {
    type Output = ScriptCacheCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            insert_counts: self.insert_counts.saturating_sub(rhs.insert_counts),
            get_counts: self.get_counts.saturating_sub(rhs.get_counts),
        }
    }
}
