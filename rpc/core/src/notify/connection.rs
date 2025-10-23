use crate::Notification;

pub type ChannelConnection = spora_notify::connection::ChannelConnection<Notification>;
pub use spora_notify::connection::ChannelType;
