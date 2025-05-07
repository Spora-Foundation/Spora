use tondi_notify::{collector::CollectorFrom, converter::ConverterFrom};
use tondi_rpc_core::Notification;

pub type WrpcServiceConverter = ConverterFrom<Notification, Notification>;
pub type WrpcServiceCollector = CollectorFrom<WrpcServiceConverter>;
