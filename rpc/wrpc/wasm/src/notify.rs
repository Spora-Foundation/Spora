//! Notification types and interfaces for wRPC events.

#![allow(non_snake_case)]

use crate::imports::*;
use spora_rpc_macros::declare_typescript_wasm_interface as declare;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEADER: &'static str = r#"

/**
 * RPC notification events.
 * 
 * @see {RpcClient.addEventListener}, {RpcClient.removeEventListener}
 */
export enum RpcEventType {
    Connect = "connect",
    Disconnect = "disconnect",
    BlockAdded = "block-added",
    VirtualChainChanged = "virtual-chain-changed",
    FinalityConflict = "finality-conflict",
    FinalityConflictResolved = "finality-conflict-resolved",
    CellsChanged = "cells-changed",
    SinkBlueScoreChanged = "sink-blue-score-changed",
    VirtualDaaScoreChanged = "virtual-daa-score-changed",
    PruningPointCellSetOverride = "pruning-point-cell-set-override",
    NewBlockTemplate = "new-block-template",
}

/**
 * RPC notification data payload.
 * 
 * @category Node RPC
 */
export type RpcEventData = IBlockAdded 
    | IVirtualChainChanged 
    | IFinalityConflict 
    | IFinalityConflictResolved 
    | ICellsChanged 
    | ISinkBlueScoreChanged 
    | IVirtualDaaScoreChanged 
    | IPruningPointCellSetOverride 
    | INewBlockTemplate;

/**
 * RPC notification event data map.
 * 
 * @category Node RPC
 */
export type RpcEventMap = {
    "connect" : undefined,
    "disconnect" : undefined,
    "block-added" : IBlockAdded,
    "virtual-chain-changed" : IVirtualChainChanged,
    "finality-conflict" : IFinalityConflict,
    "finality-conflict-resolved" : IFinalityConflictResolved,
    "cells-changed" : ICellsChanged,
    "sink-blue-score-changed" : ISinkBlueScoreChanged,
    "virtual-daa-score-changed" : IVirtualDaaScoreChanged,
    "pruning-point-cell-set-override" : IPruningPointCellSetOverride,
    "new-block-template" : INewBlockTemplate,
}

/**
 * RPC notification event.
 * 
 * @category Node RPC
 */
export type RpcEvent = {
    [K in keyof RpcEventMap]: { event: K, data: RpcEventMap[K] }
}[keyof RpcEventMap];

/**
 * RPC notification callback type.
 * 
 * This type is used to define the callback function that is called when an RPC notification is received.
 * 
 * @see {@link RpcClient.subscribeVirtualDaaScoreChanged},
 * {@link RpcClient.subscribeCellsChanged}, 
 * {@link RpcClient.subscribeVirtualChainChanged},
 * {@link RpcClient.subscribeBlockAdded},
 * {@link RpcClient.subscribeFinalityConflict},
 * {@link RpcClient.subscribeFinalityConflictResolved},
 * {@link RpcClient.subscribeSinkBlueScoreChanged},
 * {@link RpcClient.subscribePruningPointCellSetOverride},
 * {@link RpcClient.subscribeNewBlockTemplate},
 * 
 * @category Node RPC
 */
export type RpcEventCallback = (event: RpcEvent) => void;

"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "RpcEventCallback")]
    pub type RpcEventCallback;

    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "RpcEventType | string")]
    #[derive(Debug)]
    pub type RpcEventType;

    #[wasm_bindgen(typescript_type = "RpcEventType | string | RpcEventCallback")]
    #[derive(Debug)]
    pub type RpcEventTypeOrCallback;
}

declare! {
    IBlockAdded,
    r#"
    /**
     * Block added notification event is produced when a new
     * block is added to the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IBlockAdded {
        [key: string]: any;
    }
    "#,
}

declare! {
    IVirtualChainChanged,
    r#"
    /**
     * Virtual chain changed notification event is produced when the virtual
     * chain changes in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualChainChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IFinalityConflict,
    r#"
    /**
     * Finality conflict notification event is produced when a finality
     * conflict occurs in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflict {
        [key: string]: any;
    }
    "#,
}

declare! {
    IFinalityConflictResolved,
    r#"
    /**
     * Finality conflict resolved notification event is produced when a finality
     * conflict in the Spora BlockDAG is resolved.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflictResolved {
        [key: string]: any;
    }
    "#,
}

declare! {
    ICellsChanged,
    r#"
    /**
     * Cells changed notification event is produced when the set
     * of live cells changes in the
     * Spora BlockDAG. The event notification is scoped to the
     * monitored list of addresses specified during the subscription.
     * 
     * @category Node RPC
     */
    export interface ICellsChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    ISinkBlueScoreChanged,
    r#"
    /**
     * Sink blue score changed notification event is produced when the blue
     * score of the sink block changes in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface ISinkBlueScoreChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IVirtualDaaScoreChanged,
    r#"
    /**
     * Virtual DAA score changed notification event is produced when the virtual
     * Difficulty Adjustment Algorithm (DAA) score changes in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualDaaScoreChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IPruningPointCellSetOverride,
    r#"
    /**
     * Pruning point cell set override notification event is produced when the
     * cell set override for the pruning point changes in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IPruningPointCellSetOverride {
        [key: string]: any;
    }
    "#,
}

declare! {
    INewBlockTemplate,
    r#"
    /**
     * New block template notification event is produced when a new block
     * template is generated for mining in the Spora BlockDAG.
     * 
     * @category Node RPC
     */
    export interface INewBlockTemplate {
        [key: string]: any;
    }
    "#,
}
