use crate::Notification;

pub type ChannelConnection = tondi_notify::connection::ChannelConnection<Notification>;
pub use tondi_notify::connection::ChannelType;
