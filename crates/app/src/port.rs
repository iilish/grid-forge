use std::fmt;
use std::future::Future;

use tokio::sync::mpsc;

use gridforge_domain::{NewEvent, OrgScope};
use gridforge_store::{Db, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkError {
    Full,
    Closed,
}

impl SinkError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Full => "channel_full",
            Self::Closed => "channel_closed",
        }
    }
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl std::error::Error for SinkError {}

pub trait EventSink: Send + Sync {
    fn try_send(&self, event: NewEvent) -> Result<(), SinkError>;
}

pub trait EventPersister: Send + Sync {
    fn insert_event(
        &self,
        org: OrgScope,
        event: &NewEvent,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
}

impl EventPersister for Db {
    async fn insert_event(&self, org: OrgScope, event: &NewEvent) -> Result<(), StoreError> {
        Db::insert_event(self, org, event).await
    }
}

pub struct ChannelSink {
    sender: mpsc::Sender<NewEvent>,
}

impl ChannelSink {
    pub fn new(sender: mpsc::Sender<NewEvent>) -> Self {
        Self { sender }
    }
}

impl EventSink for ChannelSink {
    fn try_send(&self, event: NewEvent) -> Result<(), SinkError> {
        self.sender.try_send(event).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SinkError::Full,
            mpsc::error::TrySendError::Closed(_) => SinkError::Closed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridforge_domain::NewEvent;

    #[test]
    fn full_channel_reports_full() {
        let (sender, _receiver) = mpsc::channel(1);
        let sink = ChannelSink::new(sender);

        assert!(sink.try_send(NewEvent::test()).is_ok());
        assert_eq!(sink.try_send(NewEvent::test()), Err(SinkError::Full));
    }

    #[test]
    fn closed_channel_reports_closed() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let sink = ChannelSink::new(sender);

        assert_eq!(sink.try_send(NewEvent::test()), Err(SinkError::Closed));
    }
}
