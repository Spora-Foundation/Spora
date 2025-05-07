use crate::pb::tondid_message::Payload as TondidMessagePayload;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum TondidMessagePayloadType {
    Addresses = 0,
    Block,
    Transaction,
    BlockLocator,
    RequestAddresses,
    RequestRelayBlocks,
    RequestTransactions,
    IbdBlock,
    InvRelayBlock,
    InvTransactions,
    Ping,
    Pong,
    Verack,
    Version,
    TransactionNotFound,
    Reject,
    PruningPointUtxoSetChunk,
    RequestIbdBlocks,
    UnexpectedPruningPoint,
    IbdBlockLocator,
    IbdBlockLocatorHighestHash,
    RequestNextPruningPointUtxoSetChunk,
    DonePruningPointUtxoSetChunks,
    IbdBlockLocatorHighestHashNotFound,
    BlockWithTrustedData,
    DoneBlocksWithTrustedData,
    RequestPruningPointAndItsAnticone,
    BlockHeaders,
    RequestNextHeaders,
    DoneHeaders,
    RequestPruningPointUtxoSet,
    RequestHeaders,
    RequestBlockLocator,
    PruningPoints,
    RequestPruningPointProof,
    PruningPointProof,
    Ready,
    BlockWithTrustedDataV4,
    TrustedData,
    RequestIbdChainBlockLocator,
    IbdChainBlockLocator,
    RequestAntipast,
    RequestNextPruningPointAndItsAnticoneBlocks,
}

impl From<&TondidMessagePayload> for TondidMessagePayloadType {
    fn from(payload: &TondidMessagePayload) -> Self {
        match payload {
            TondidMessagePayload::Addresses(_) => TondidMessagePayloadType::Addresses,
            TondidMessagePayload::Block(_) => TondidMessagePayloadType::Block,
            TondidMessagePayload::Transaction(_) => TondidMessagePayloadType::Transaction,
            TondidMessagePayload::BlockLocator(_) => TondidMessagePayloadType::BlockLocator,
            TondidMessagePayload::RequestAddresses(_) => TondidMessagePayloadType::RequestAddresses,
            TondidMessagePayload::RequestRelayBlocks(_) => TondidMessagePayloadType::RequestRelayBlocks,
            TondidMessagePayload::RequestTransactions(_) => TondidMessagePayloadType::RequestTransactions,
            TondidMessagePayload::IbdBlock(_) => TondidMessagePayloadType::IbdBlock,
            TondidMessagePayload::InvRelayBlock(_) => TondidMessagePayloadType::InvRelayBlock,
            TondidMessagePayload::InvTransactions(_) => TondidMessagePayloadType::InvTransactions,
            TondidMessagePayload::Ping(_) => TondidMessagePayloadType::Ping,
            TondidMessagePayload::Pong(_) => TondidMessagePayloadType::Pong,
            TondidMessagePayload::Verack(_) => TondidMessagePayloadType::Verack,
            TondidMessagePayload::Version(_) => TondidMessagePayloadType::Version,
            TondidMessagePayload::TransactionNotFound(_) => TondidMessagePayloadType::TransactionNotFound,
            TondidMessagePayload::Reject(_) => TondidMessagePayloadType::Reject,
            TondidMessagePayload::PruningPointUtxoSetChunk(_) => TondidMessagePayloadType::PruningPointUtxoSetChunk,
            TondidMessagePayload::RequestIbdBlocks(_) => TondidMessagePayloadType::RequestIbdBlocks,
            TondidMessagePayload::UnexpectedPruningPoint(_) => TondidMessagePayloadType::UnexpectedPruningPoint,
            TondidMessagePayload::IbdBlockLocator(_) => TondidMessagePayloadType::IbdBlockLocator,
            TondidMessagePayload::IbdBlockLocatorHighestHash(_) => TondidMessagePayloadType::IbdBlockLocatorHighestHash,
            TondidMessagePayload::RequestNextPruningPointUtxoSetChunk(_) => {
                TondidMessagePayloadType::RequestNextPruningPointUtxoSetChunk
            }
            TondidMessagePayload::DonePruningPointUtxoSetChunks(_) => TondidMessagePayloadType::DonePruningPointUtxoSetChunks,
            TondidMessagePayload::IbdBlockLocatorHighestHashNotFound(_) => {
                TondidMessagePayloadType::IbdBlockLocatorHighestHashNotFound
            }
            TondidMessagePayload::BlockWithTrustedData(_) => TondidMessagePayloadType::BlockWithTrustedData,
            TondidMessagePayload::DoneBlocksWithTrustedData(_) => TondidMessagePayloadType::DoneBlocksWithTrustedData,
            TondidMessagePayload::RequestPruningPointAndItsAnticone(_) => TondidMessagePayloadType::RequestPruningPointAndItsAnticone,
            TondidMessagePayload::BlockHeaders(_) => TondidMessagePayloadType::BlockHeaders,
            TondidMessagePayload::RequestNextHeaders(_) => TondidMessagePayloadType::RequestNextHeaders,
            TondidMessagePayload::DoneHeaders(_) => TondidMessagePayloadType::DoneHeaders,
            TondidMessagePayload::RequestPruningPointUtxoSet(_) => TondidMessagePayloadType::RequestPruningPointUtxoSet,
            TondidMessagePayload::RequestHeaders(_) => TondidMessagePayloadType::RequestHeaders,
            TondidMessagePayload::RequestBlockLocator(_) => TondidMessagePayloadType::RequestBlockLocator,
            TondidMessagePayload::PruningPoints(_) => TondidMessagePayloadType::PruningPoints,
            TondidMessagePayload::RequestPruningPointProof(_) => TondidMessagePayloadType::RequestPruningPointProof,
            TondidMessagePayload::PruningPointProof(_) => TondidMessagePayloadType::PruningPointProof,
            TondidMessagePayload::Ready(_) => TondidMessagePayloadType::Ready,
            TondidMessagePayload::BlockWithTrustedDataV4(_) => TondidMessagePayloadType::BlockWithTrustedDataV4,
            TondidMessagePayload::TrustedData(_) => TondidMessagePayloadType::TrustedData,
            TondidMessagePayload::RequestIbdChainBlockLocator(_) => TondidMessagePayloadType::RequestIbdChainBlockLocator,
            TondidMessagePayload::IbdChainBlockLocator(_) => TondidMessagePayloadType::IbdChainBlockLocator,
            TondidMessagePayload::RequestAntipast(_) => TondidMessagePayloadType::RequestAntipast,
            TondidMessagePayload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                TondidMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks
            }
        }
    }
}
