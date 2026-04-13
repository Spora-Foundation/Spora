//!
//! Conversion functions for converting between
//! native [`spora_consensus_core`] types and [`spora_wallet_psst`](crate) types.
//!

use crate::error::Error;
use crate::input::{Input, InputBuilder};
use crate::output::{Output, OutputBuilder};
use crate::psst::{Global, Inner};
use spora_consensus_core::cell_diff::CellMeta;
use spora_consensus_core::tx::{self as cctx};

fn output_from_cell_out(output: &cctx::CellOut, output_data: &[u8]) -> Output {
    OutputBuilder::default()
        .capacity(output.capacity)
        .lock_script(output.lock.clone())
        .output_data(output_data.to_vec())
        .build()
        .map(|mut built| {
            built.type_script = output.type_.clone();
            built
        })
        .expect("CellOut must map to Output")
}

impl TryFrom<(cctx::CellTx, Vec<(cctx::CellRef, CellMeta)>)> for Inner {
    type Error = Error; // Define your error type

    fn try_from((transaction, inputs_with_entries): (cctx::CellTx, Vec<(cctx::CellRef, CellMeta)>)) -> Result<Self, Self::Error> {
        let inputs: Result<Vec<Input>, Self::Error> = inputs_with_entries
            .into_iter()
            .map(|(cell_ref, cell_entry)| {
                let since = cell_ref.since;
                let mut built = InputBuilder::default()
                    .cell_entry(cell_entry)
                    .previous_outpoint(cell_ref.out_point)
                    .build()
                    .map_err(Error::TxToInnerConversionInputBuildingError)?;
                built.since = Some(since);
                Ok(built)
            })
            .collect::<Result<_, _>>();

        let outputs = transaction
            .outputs
            .iter()
            .enumerate()
            .map(|(index, output)| {
                output_from_cell_out(output, transaction.outputs_data.get(index).map(Vec::as_slice).unwrap_or_default())
            })
            .collect::<Vec<_>>();

        Ok(Inner { global: Global::default(), inputs: inputs?, outputs })
    }
}

impl TryFrom<cctx::CellTx> for Inner {
    type Error = Error;
    fn try_from(transaction: cctx::CellTx) -> Result<Self, self::Error> {
        let inputs = transaction
            .inputs
            .iter()
            .map(|input| -> Result<Input, Error> {
                let mut built = InputBuilder::default()
                    .previous_outpoint(input.out_point)
                    .build()
                    .map_err(Error::TxToInnerConversionInputBuildingError)?;
                built.since = Some(input.since);
                Ok(built)
            })
            .collect::<Result<_, _>>()?;

        let outputs = transaction
            .outputs
            .iter()
            .enumerate()
            .map(|(index, output)| {
                output_from_cell_out(output, transaction.outputs_data.get(index).map(Vec::as_slice).unwrap_or_default())
            })
            .collect::<Vec<_>>();

        Ok(Inner { global: Global::default(), inputs, outputs })
    }
}
