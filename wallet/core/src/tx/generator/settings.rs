//!
//! Transaction [`GeneratorSettings`] used when
//! constructing and instance of the [`Generator`](crate::tx::Generator).
//!

use crate::cell::{CellContext, CellEntryReference, CellIterator};
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::tx::{Fees, PaymentDestination};
use spora_addresses::Address;
use workflow_core::channel::Multiplexer;

pub struct GeneratorSettings {
    // Network type
    pub network_id: NetworkId,
    // Event multiplexer
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    // Cell iterator
    pub cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>,
    // Source cell context
    pub source_cell_context: Option<CellContext>,
    // Priority cell entries that are consumed before others
    pub priority_cell_entries: Option<Vec<CellEntryReference>>,
    // number of minimum signatures required to sign the transaction
    pub minimum_signatures: u16,
    // change address
    pub change_address: Address,
    // fee rate
    #[allow(dead_code)]
    pub fee_rate: Option<f64>,
    // applies only to the final transaction
    pub final_transaction_priority_fee: Fees,
    // final transaction outputs
    pub final_transaction_destination: PaymentDestination,
    // payload
    pub final_transaction_payload: Option<Vec<u8>>,
    // CellScript compiled scheduler witness bytes for the final transaction.
    pub final_cellscript_compiled_scheduler_witness: Option<Vec<u8>>,
    // transaction is a transfer between accounts
    pub destination_cell_context: Option<CellContext>,
}

// impl std::fmt::Debug for GeneratorSettings {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("GeneratorSettings")
//             .field("network_id", &self.network_id)
//             // .field("multiplexer", &self.multiplexer)
//             .field("cell_iterator", &"Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>")
//             // .field("source_cell_context", &self.source_cell_context)
//             .field("minimum_signatures", &self.minimum_signatures)
//             .field("change_address", &self.change_address)
//             .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
//             .field("final_transaction_destination", &self.final_transaction_destination)
//             .field("final_transaction_payload", &self.final_transaction_payload)
//             // .field("destination_cell_context", &self.destination_cell_context)
//             .finish()
//     }
// }

impl GeneratorSettings {
    pub fn try_new_with_account(
        account: Arc<dyn Account>,
        final_transaction_destination: PaymentDestination,
        _fee_rate: Option<f64>,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let network_id = account.cell_context().processor().network_id()?;
        let change_address = account.change_address()?;
        let multiplexer = account.wallet().multiplexer().clone();
        let minimum_signatures = account.minimum_signatures();

        let cell_iterator = CellIterator::new(account.cell_context());

        let settings = GeneratorSettings {
            network_id,
            multiplexer: Some(multiplexer),
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: Some(account.cell_context().clone()),
            priority_cell_entries: None,
            fee_rate: None,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            destination_cell_context: None,
        };

        Ok(settings)
    }

    pub fn try_new_with_context(
        cell_context: CellContext,
        priority_cell_entries: Option<Vec<CellEntryReference>>,
        change_address: Address,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let network_id = cell_context.processor().network_id()?;
        let cell_iterator = CellIterator::new(&cell_context);

        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: Some(cell_context),
            priority_cell_entries,
            fee_rate: None,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            destination_cell_context: None,
        };

        Ok(settings)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new_with_iterator(
        network_id: NetworkId,
        cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>,
        priority_cell_entries: Option<Vec<CellEntryReference>>,
        change_address: Address,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        _fee_rate: Option<f64>,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: None,
            priority_cell_entries,
            fee_rate: _fee_rate,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            destination_cell_context: None,
        };

        Ok(settings)
    }

    pub fn with_cellscript_compiled_scheduler_witness(mut self, witness: Vec<u8>) -> Self {
        self.final_cellscript_compiled_scheduler_witness = Some(witness);
        self
    }

    pub fn cell_context_transfer(mut self, destination_cell_context: &CellContext) -> Self {
        self.destination_cell_context = Some(destination_cell_context.clone());
        self
    }
}
