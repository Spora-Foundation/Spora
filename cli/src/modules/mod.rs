use crate::imports::*;

pub mod account;
pub mod address;
pub mod broadcast;
pub mod c;
pub mod close;
pub mod connect;

#[path = "create-unsigned-tx.rs"]
pub mod create_unsigned_tx;
pub mod details;
pub mod disconnect;
pub mod pretty;
pub mod estimate;
pub mod exit;
pub mod export;
pub mod guide;
pub mod halt;
pub mod help;
// pub mod import;
pub mod list;
pub mod ls;
pub mod message;
pub mod miner;
pub mod monitor;
pub mod mute;
pub mod network;
pub mod node;
pub mod open;
pub mod ping;
pub mod pstb;
pub mod reload;
pub mod rpc;
pub mod select;
pub mod send;
pub mod server;
pub mod settings;
pub mod sign;
pub mod start;
pub mod stop;
pub mod sweep;
// pub mod test;
pub mod track;
pub mod transfer;
pub mod wallet;
pub mod clear;

// this module is registered manually within
// applications that support metrics
pub mod metrics;

// TODO
// broadcast
// create-unsigned-tx
// sign

pub fn register_handlers(cli: &Arc<TondiCli>) -> Result<()> {
    register_handlers!(
        cli,
        cli.handlers(),
        [
            account, address, c, clear, close, connect, details, disconnect, pretty, estimate, exit, export, guide, help, ls, rpc, list, miner,
            message, monitor, mute, network, node, open, ping, pstb, reload, select, send, server, settings, sign, start, stop, sweep, track, transfer,
            wallet,
            // halt,
            // theme,  start, stop
        ]
    );

    Ok(())
}
