// Test utilities module - uses deprecated legacy types for backward compatibility with existing tests
#![cfg(test)]
#![allow(deprecated)]

pub(super) mod coinbase_mock;
pub(crate) mod consensus_mock;
pub(crate) mod legacy_script;
