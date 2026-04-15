#[cfg(feature = "heap")]
#[global_allocator]
#[cfg(not(feature = "heap"))]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub mod common;
#[cfg(feature = "integration-tests")]
pub mod tasks;

#[cfg(test)]
#[cfg(feature = "cellindex-tests")]
pub mod consensus_integration_tests;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
pub mod consensus_pipeline_tests;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
pub mod daemon_integration_tests;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod mempool_benchmarks;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod subscribe_benchmarks;

#[cfg(test)]
pub mod rpc_tests;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
pub mod wallet_account_variant_tests;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
pub mod consensus_mempool_template_matrix_tests;
