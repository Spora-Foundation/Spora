use crate::{flow_context::FlowContext, flow_trait::Flow, v5::ibd::IBD_BATCH_SIZE};
use itertools::Itertools;
use spora_consensus_core::errors::consensus::ConsensusError;
use spora_core::debug;
use spora_hashes::Hash;
use spora_p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{p2p_message::Payload, DonePruningPointCellSetChunksMessage, PruningPointCellSetChunkMessage, UnexpectedPruningPointMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;

pub struct RequestPruningPointCellSetFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointCellSetFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointCellSetFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let expected_pp = dequeue!(self.incoming_route, Payload::RequestPruningPointCellSet)?.try_into()?;
            self.handle_request(expected_pp).await?
        }
    }

    async fn handle_request(&mut self, expected_pp: Hash) -> Result<(), ProtocolError> {
        const CHUNK_SIZE: usize = 1000;
        let mut from_outpoint = None;
        let mut chunks_sent = 0;

        let consensus = self.ctx.consensus();
        let mut session = consensus.session().await;

        loop {
            // We avoid keeping the consensus session across the limitless dequeue call below
            let pruning_point_cells =
                match session.async_get_pruning_point_cells(expected_pp, from_outpoint, CHUNK_SIZE, chunks_sent != 0).await {
                    Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point_message().await,
                    res => res,
                }?;
            debug!("Retrieved {} Cells for pruning point {}", pruning_point_cells.len(), expected_pp);

            // Send the chunk
            self.router
                .enqueue(make_message!(
                    Payload::PruningPointCellSetChunk,
                    PruningPointCellSetChunkMessage {
                        outpoint_and_cell_entry_pairs: pruning_point_cells
                            .iter()
                            .map(|(outpoint, entry)| { (outpoint, entry).into() })
                            .collect_vec()
                    }
                ))
                .await?;

            chunks_sent += 1;
            if chunks_sent % IBD_BATCH_SIZE == 0 {
                drop(session); // Avoid holding the session through dequeue calls
                dequeue!(self.incoming_route, Payload::RequestNextPruningPointCellSetChunk)?;
                session = consensus.session().await;
            }

            // This indicates that there are no more entries to query
            if pruning_point_cells.len() < CHUNK_SIZE {
                return self.send_done_message(expected_pp).await;
            }

            // Mark the beginning of the next chunk
            from_outpoint = Some(pruning_point_cells.last().expect("not empty by prev condition").0);
        }
    }

    async fn send_unexpected_pruning_point_message(&mut self) -> Result<(), ProtocolError> {
        self.router.enqueue(make_message!(Payload::UnexpectedPruningPoint, UnexpectedPruningPointMessage {})).await?;
        Ok(())
    }

    async fn send_done_message(&mut self, expected_pp: Hash) -> Result<(), ProtocolError> {
        debug!("Finished sending Cells for pruning point {}", expected_pp);
        self.router.enqueue(make_message!(Payload::DonePruningPointCellSetChunks, DonePruningPointCellSetChunksMessage {})).await?;
        Ok(())
    }
}
