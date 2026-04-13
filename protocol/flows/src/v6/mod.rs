use crate::v5::{
    address::{ReceiveAddressesFlow, SendAddressesFlow},
    blockrelay::{flow::HandleRelayInvsFlow, handle_requests::HandleRelayBlockRequests},
    ibd::IbdFlow,
    ping::{ReceivePingsFlow, SendPingsFlow},
    request_antipast::HandleAntipastRequests,
    request_block_locator::RequestBlockLocatorFlow,
    request_headers::RequestHeadersFlow,
    request_ibd_blocks::HandleIbdBlockRequests,
    request_ibd_chain_block_locator::RequestIbdChainBlockLocatorFlow,
    request_pp_proof::RequestPruningPointProofFlow,
    request_pruning_point_cell_set::RequestPruningPointCellSetFlow,
    txrelay::flow::{RelayTransactionsFlow, RequestTransactionsFlow},
};
use crate::{flow_context::FlowContext, flow_trait::Flow};

use spora_p2p_lib::{P2pMessagePayloadType, Router, SharedIncomingRoute};
use spora_utils::channel;
use std::sync::Arc;

use crate::v6::request_pruning_point_and_anticone::PruningPointAndItsAnticoneRequestsFlow;

pub(crate) mod request_pruning_point_and_anticone;

pub fn register(ctx: FlowContext, router: Arc<Router>) -> Vec<Box<dyn Flow>> {
    // IBD flow <-> invs flow communication uses a job channel in order to always
    // maintain at most a single pending job which can be updated
    let (ibd_sender, relay_receiver) = channel::job();

    let mut flows: Vec<Box<dyn Flow>> = vec![
        Box::new(IbdFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                P2pMessagePayloadType::BlockHeaders,
                P2pMessagePayloadType::DoneHeaders,
                P2pMessagePayloadType::IbdBlockLocatorHighestHash,
                P2pMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
                P2pMessagePayloadType::BlockWithTrustedDataV4,
                P2pMessagePayloadType::DoneBlocksWithTrustedData,
                P2pMessagePayloadType::IbdChainBlockLocator,
                P2pMessagePayloadType::IbdBlock,
                P2pMessagePayloadType::TrustedData,
                P2pMessagePayloadType::PruningPoints,
                P2pMessagePayloadType::PruningPointProof,
                P2pMessagePayloadType::UnexpectedPruningPoint,
                P2pMessagePayloadType::PruningPointCellSetChunk,
                P2pMessagePayloadType::DonePruningPointCellSetChunks,
            ]),
            relay_receiver,
        )),
        Box::new(HandleRelayBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestRelayBlocks]),
        )),
        Box::new(ReceivePingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![P2pMessagePayloadType::Ping]))),
        Box::new(SendPingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![P2pMessagePayloadType::Pong]))),
        Box::new(RequestHeadersFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestHeaders, P2pMessagePayloadType::RequestNextHeaders]),
        )),
        Box::new(RequestPruningPointProofFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestPruningPointProof]),
        )),
        Box::new(RequestIbdChainBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestIbdChainBlockLocator]),
        )),
        Box::new(PruningPointAndItsAnticoneRequestsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                P2pMessagePayloadType::RequestPruningPointAndItsAnticone,
                P2pMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
            ]),
        )),
        Box::new(RequestPruningPointCellSetFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                P2pMessagePayloadType::RequestPruningPointCellSet,
                P2pMessagePayloadType::RequestNextPruningPointCellSetChunk,
            ]),
        )),
        Box::new(HandleIbdBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestIbdBlocks]),
        )),
        Box::new(HandleAntipastRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestAntipast]),
        )),
        Box::new(RelayTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe_with_capacity(vec![P2pMessagePayloadType::InvTransactions], RelayTransactionsFlow::invs_channel_size()),
            router.subscribe_with_capacity(
                vec![P2pMessagePayloadType::Transaction, P2pMessagePayloadType::TransactionNotFound],
                RelayTransactionsFlow::txs_channel_size(),
            ),
        )),
        Box::new(RequestTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestTransactions]),
        )),
        Box::new(ReceiveAddressesFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![P2pMessagePayloadType::Addresses]))),
        Box::new(SendAddressesFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![P2pMessagePayloadType::RequestAddresses]))),
        Box::new(RequestBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![P2pMessagePayloadType::RequestBlockLocator]),
        )),
    ];

    let invs_route = router.subscribe_with_capacity(vec![P2pMessagePayloadType::InvRelayBlock], ctx.block_invs_channel_size());
    let shared_invs_route = SharedIncomingRoute::new(invs_route);

    let num_relay_flows = (ctx.config.bps() as usize / 2).max(1);
    flows.extend((0..num_relay_flows).map(|_| {
        Box::new(HandleRelayInvsFlow::new(
            ctx.clone(),
            router.clone(),
            shared_invs_route.clone(),
            router.subscribe(vec![]),
            ibd_sender.clone(),
        )) as Box<dyn Flow>
    }));

    // The reject message is handled as a special case by the router
    // P2pMessagePayloadType::Reject,

    // We do not register the below two messages since they are deprecated also in go-spora
    // P2pMessagePayloadType::BlockWithTrustedData,
    // P2pMessagePayloadType::IbdBlockLocator,

    flows
}
