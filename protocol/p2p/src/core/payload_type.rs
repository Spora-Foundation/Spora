use crate::pb::tondid_message::Payload as SporadMessagePayload;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum SporadMessagePayloadType {
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

impl From<&SporadMessagePayload> for SporadMessagePayloadType {
    fn from(payload: &SporadMessagePayload) -> Self {
        match payload {
            SporadMessagePayload::Addresses(_) => SporadMessagePayloadType::Addresses,
            SporadMessagePayload::Block(_) => SporadMessagePayloadType::Block,
            SporadMessagePayload::Transaction(_) => SporadMessagePayloadType::Transaction,
            SporadMessagePayload::BlockLocator(_) => SporadMessagePayloadType::BlockLocator,
            SporadMessagePayload::RequestAddresses(_) => SporadMessagePayloadType::RequestAddresses,
            SporadMessagePayload::RequestRelayBlocks(_) => SporadMessagePayloadType::RequestRelayBlocks,
            SporadMessagePayload::RequestTransactions(_) => SporadMessagePayloadType::RequestTransactions,
            SporadMessagePayload::IbdBlock(_) => SporadMessagePayloadType::IbdBlock,
            SporadMessagePayload::InvRelayBlock(_) => SporadMessagePayloadType::InvRelayBlock,
            SporadMessagePayload::InvTransactions(_) => SporadMessagePayloadType::InvTransactions,
            SporadMessagePayload::Ping(_) => SporadMessagePayloadType::Ping,
            SporadMessagePayload::Pong(_) => SporadMessagePayloadType::Pong,
            SporadMessagePayload::Verack(_) => SporadMessagePayloadType::Verack,
            SporadMessagePayload::Version(_) => SporadMessagePayloadType::Version,
            SporadMessagePayload::TransactionNotFound(_) => SporadMessagePayloadType::TransactionNotFound,
            SporadMessagePayload::Reject(_) => SporadMessagePayloadType::Reject,
            SporadMessagePayload::PruningPointUtxoSetChunk(_) => SporadMessagePayloadType::PruningPointUtxoSetChunk,
            SporadMessagePayload::RequestIbdBlocks(_) => SporadMessagePayloadType::RequestIbdBlocks,
            SporadMessagePayload::UnexpectedPruningPoint(_) => SporadMessagePayloadType::UnexpectedPruningPoint,
            SporadMessagePayload::IbdBlockLocator(_) => SporadMessagePayloadType::IbdBlockLocator,
            SporadMessagePayload::IbdBlockLocatorHighestHash(_) => SporadMessagePayloadType::IbdBlockLocatorHighestHash,
            SporadMessagePayload::RequestNextPruningPointUtxoSetChunk(_) => {
                SporadMessagePayloadType::RequestNextPruningPointUtxoSetChunk
            }
            SporadMessagePayload::DonePruningPointUtxoSetChunks(_) => SporadMessagePayloadType::DonePruningPointUtxoSetChunks,
            SporadMessagePayload::IbdBlockLocatorHighestHashNotFound(_) => {
                SporadMessagePayloadType::IbdBlockLocatorHighestHashNotFound
            }
            SporadMessagePayload::BlockWithTrustedData(_) => SporadMessagePayloadType::BlockWithTrustedData,
            SporadMessagePayload::DoneBlocksWithTrustedData(_) => SporadMessagePayloadType::DoneBlocksWithTrustedData,
            SporadMessagePayload::RequestPruningPointAndItsAnticone(_) => SporadMessagePayloadType::RequestPruningPointAndItsAnticone,
            SporadMessagePayload::BlockHeaders(_) => SporadMessagePayloadType::BlockHeaders,
            SporadMessagePayload::RequestNextHeaders(_) => SporadMessagePayloadType::RequestNextHeaders,
            SporadMessagePayload::DoneHeaders(_) => SporadMessagePayloadType::DoneHeaders,
            SporadMessagePayload::RequestPruningPointUtxoSet(_) => SporadMessagePayloadType::RequestPruningPointUtxoSet,
            SporadMessagePayload::RequestHeaders(_) => SporadMessagePayloadType::RequestHeaders,
            SporadMessagePayload::RequestBlockLocator(_) => SporadMessagePayloadType::RequestBlockLocator,
            SporadMessagePayload::PruningPoints(_) => SporadMessagePayloadType::PruningPoints,
            SporadMessagePayload::RequestPruningPointProof(_) => SporadMessagePayloadType::RequestPruningPointProof,
            SporadMessagePayload::PruningPointProof(_) => SporadMessagePayloadType::PruningPointProof,
            SporadMessagePayload::Ready(_) => SporadMessagePayloadType::Ready,
            SporadMessagePayload::BlockWithTrustedDataV4(_) => SporadMessagePayloadType::BlockWithTrustedDataV4,
            SporadMessagePayload::TrustedData(_) => SporadMessagePayloadType::TrustedData,
            SporadMessagePayload::RequestIbdChainBlockLocator(_) => SporadMessagePayloadType::RequestIbdChainBlockLocator,
            SporadMessagePayload::IbdChainBlockLocator(_) => SporadMessagePayloadType::IbdChainBlockLocator,
            SporadMessagePayload::RequestAntipast(_) => SporadMessagePayloadType::RequestAntipast,
            SporadMessagePayload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                SporadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks
            }
        }
    }
}
