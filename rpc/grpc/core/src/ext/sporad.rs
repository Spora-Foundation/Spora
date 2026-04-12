use spora_notify::{scope::Scope, subscription::Command};

use crate::protowire::{
    sporad_request, sporad_response, NotifyBlockAddedRequestMessage, NotifyCellsChangedRequestMessage,
    NotifyFinalityConflictRequestMessage, NotifyNewBlockTemplateRequestMessage, NotifyPruningPointCellSetOverrideRequestMessage,
    NotifySinkBlueScoreChangedRequestMessage, NotifyVirtualChainChangedRequestMessage, NotifyVirtualDaaScoreChangedRequestMessage,
    SporadRequest, SporadResponse,
};

impl SporadRequest {
    pub fn from_notification_type(scope: &Scope, command: Command) -> Self {
        SporadRequest { id: 0, payload: Some(sporad_request::Payload::from_notification_type(scope, command)) }
    }

    pub fn is_subscription(&self) -> bool {
        self.payload.as_ref().is_some_and(|x| x.is_subscription())
    }
}

impl sporad_request::Payload {
    pub fn from_notification_type(scope: &Scope, command: Command) -> Self {
        match scope {
            Scope::BlockAdded(_) => {
                sporad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            }
            Scope::NewBlockTemplate(_) => {
                sporad_request::Payload::NotifyNewBlockTemplateRequest(NotifyNewBlockTemplateRequestMessage {
                    command: command.into(),
                })
            }

            Scope::VirtualChainChanged(ref scope) => {
                sporad_request::Payload::NotifyVirtualChainChangedRequest(NotifyVirtualChainChangedRequestMessage {
                    command: command.into(),
                    include_accepted_transaction_ids: scope.include_accepted_transaction_ids,
                })
            }
            Scope::FinalityConflict(_) => {
                sporad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            Scope::FinalityConflictResolved(_) => {
                sporad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            Scope::CellsChanged(ref scope) => sporad_request::Payload::NotifyCellsChangedRequest(NotifyCellsChangedRequestMessage {
                addresses: scope.addresses.iter().map(|x| x.into()).collect::<Vec<String>>(),
                command: command.into(),
            }),
            Scope::SinkBlueScoreChanged(_) => {
                sporad_request::Payload::NotifySinkBlueScoreChangedRequest(NotifySinkBlueScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            Scope::VirtualDaaScoreChanged(_) => {
                sporad_request::Payload::NotifyVirtualDaaScoreChangedRequest(NotifyVirtualDaaScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            Scope::PruningPointCellSetOverride(_) => {
                sporad_request::Payload::NotifyPruningPointCellSetOverrideRequest(NotifyPruningPointCellSetOverrideRequestMessage {
                    command: command.into(),
                })
            }
        }
    }

    pub fn is_subscription(&self) -> bool {
        use crate::protowire::sporad_request::Payload;
        matches!(
            self,
            Payload::NotifyBlockAddedRequest(_)
                | Payload::NotifyVirtualChainChangedRequest(_)
                | Payload::NotifyFinalityConflictRequest(_)
                | Payload::NotifyCellsChangedRequest(_)
                | Payload::NotifySinkBlueScoreChangedRequest(_)
                | Payload::NotifyVirtualDaaScoreChangedRequest(_)
                | Payload::NotifyPruningPointCellSetOverrideRequest(_)
                | Payload::NotifyNewBlockTemplateRequest(_)
                | Payload::StopNotifyingCellsChangedRequest(_)
                | Payload::StopNotifyingPruningPointCellSetOverrideRequest(_)
        )
    }
}

impl SporadResponse {
    pub fn is_notification(&self) -> bool {
        match self.payload {
            Some(ref payload) => payload.is_notification(),
            None => false,
        }
    }
}

#[allow(clippy::match_like_matches_macro)]
impl sporad_response::Payload {
    pub fn is_notification(&self) -> bool {
        use crate::protowire::sporad_response::Payload;
        match self {
            Payload::BlockAddedNotification(_) => true,
            Payload::VirtualChainChangedNotification(_) => true,
            Payload::FinalityConflictNotification(_) => true,
            Payload::FinalityConflictResolvedNotification(_) => true,
            Payload::CellsChangedNotification(_) => true,
            Payload::SinkBlueScoreChangedNotification(_) => true,
            Payload::VirtualDaaScoreChangedNotification(_) => true,
            Payload::PruningPointCellSetOverrideNotification(_) => true,
            Payload::NewBlockTemplateNotification(_) => true,
            _ => false,
        }
    }
}
