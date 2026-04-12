//!
//! Logical stream abstractions used throughout the IBD negotiation protocols
//!

use spora_consensus_core::{cell_diff::CellMeta, errors::consensus::ConsensusError, header::Header, tx::TransactionOutpoint};
use spora_core::{debug, info};
use spora_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    convert::model::trusted::TrustedDataEntry,
    make_message,
    pb::{
        sporad_message::Payload, RequestNextHeadersMessage, RequestNextPruningPointAndItsAnticoneBlocksMessage,
        RequestNextPruningPointCellSetChunkMessage,
    },
    IncomingRoute, Router,
};
use std::sync::Arc;
use tokio::time::timeout;

pub const IBD_BATCH_SIZE: usize = 99;

pub struct TrustedEntryStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize,
}

impl<'a, 'b> TrustedEntryStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<TrustedDataEntry>, ProtocolError> {
        let res = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockWithTrustedDataV4(payload)) => {
                            let entry: TrustedDataEntry = payload.try_into()?;
                            if entry.block.is_header_only() {
                                Err(ProtocolError::OtherOwned(format!("trusted entry block {} is header only", entry.block.hash())))
                            } else {
                                Ok(Some(entry))
                            }
                        }
                        Some(Payload::DoneBlocksWithTrustedData(_)) => {
                            debug!("trusted entry stream completed after {} items", self.i);
                            Ok(None)
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockWithTrustedDataV4 | Payload::DoneBlocksWithTrustedData),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(_)) = res {
            self.i += 1;
            if self.i % IBD_BATCH_SIZE == 0 {
                info!("Downloaded {} blocks from the pruning point anticone", self.i - 1);
                self.router
                    .enqueue(make_message!(
                        Payload::RequestNextPruningPointAndItsAnticoneBlocks,
                        RequestNextPruningPointAndItsAnticoneBlocksMessage {}
                    ))
                    .await?;
            }
        }

        res
    }
}

/// A chunk of headers
pub type HeadersChunk = Vec<Arc<Header>>;

pub struct HeadersChunkStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize,
}

impl<'a, 'b> HeadersChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<HeadersChunk>, ProtocolError> {
        let res = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockHeaders(payload)) => {
                            if payload.block_headers.is_empty() {
                                // The syncer should have sent a done message if the search completed, and not an empty list
                                Err(ProtocolError::Other("Received an empty headers message"))
                            } else {
                                Ok(Some(payload.try_into()?))
                            }
                        }
                        Some(Payload::DoneHeaders(_)) => {
                            debug!("headers chunk stream completed after {} chunks", self.i);
                            Ok(None)
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockHeaders | Payload::DoneHeaders),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(_)) = res {
            self.i += 1;
            self.router.enqueue(make_message!(Payload::RequestNextHeaders, RequestNextHeadersMessage {})).await?;
        }

        res
    }
}

/// A chunk of Cells (Cell model)
pub type CellsetChunk = Vec<(TransactionOutpoint, CellMeta)>;

pub struct PruningPointCellsetChunkStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize, // Chunk index
    cell_count: usize,
}

impl<'a, 'b> PruningPointCellsetChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0, cell_count: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<CellsetChunk>, ProtocolError> {
        let res: Result<Option<CellsetChunk>, ProtocolError> = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::PruningPointCellSetChunk(payload)) => {
                            let entry_chunk: Vec<(TransactionOutpoint, spora_consensus_core::tx::CellEntry)> = payload.try_into()?;
                            let cell_chunk: Vec<_> = entry_chunk
                                .into_iter()
                                .map(|(outpoint, cell_entry)| {
                                    let mut cell_meta = cell_entry;
                                    cell_meta.out_point = outpoint.clone();
                                    (outpoint, cell_meta)
                                })
                                .collect();
                            Ok(Some(cell_chunk))
                        }
                        Some(Payload::DonePruningPointCellSetChunks(_)) => {
                            info!("Finished receiving the Cell set. Total Cells: {}", self.cell_count);
                            Ok(None)
                        }
                        Some(Payload::UnexpectedPruningPoint(_)) => {
                            // Although this can happen also to an honest syncer (if his pruning point moves during the sync),
                            // we prefer erring and disconnecting to avoid possible exploits by a syncer repeating this failure
                            Err(ProtocolError::ConsensusError(ConsensusError::UnexpectedPruningPoint))
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(
                                Payload::PruningPointCellSetChunk
                                    | Payload::DonePruningPointCellSetChunks
                                    | Payload::UnexpectedPruningPoint
                            ),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(chunk)) = res {
            self.i += 1;
            self.cell_count += chunk.len();
            if self.i % IBD_BATCH_SIZE == 0 {
                info!("Received {} Cell set chunks so far, totaling in {} Cells", self.i, self.cell_count);
                self.router
                    .enqueue(make_message!(
                        Payload::RequestNextPruningPointCellSetChunk,
                        RequestNextPruningPointCellSetChunkMessage {}
                    ))
                    .await?;
            }
            Ok(Some(chunk))
        } else {
            res
        }
    }
}
