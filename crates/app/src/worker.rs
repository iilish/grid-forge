use std::sync::Arc;
use std::time::Duration;

use gridforge_domain::{NewEvent, OrgScope};
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::metrics::{DropReason, Metrics};
use crate::port::EventPersister;

const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistOutcome {
    Transient,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAction {
    Retry { delay_ms: u64 },
    Drop,
}

pub fn retry_next(attempt: u32, outcome: PersistOutcome) -> RetryAction {
    match outcome {
        PersistOutcome::Transient if attempt < MAX_ATTEMPTS => RetryAction::Retry {
            delay_ms: 100 * u64::from(attempt),
        },
        _ => RetryAction::Drop,
    }
}

pub async fn run<P>(
    mut receiver: mpsc::Receiver<NewEvent>,
    persister: P,
    metrics: Arc<Metrics>,
    org: OrgScope,
) where
    P: EventPersister,
{
    while let Some(event) = receiver.recv().await {
        let mut persisted = false;

        for attempt in 1..=MAX_ATTEMPTS {
            match persister.insert_event(org, &event).await {
                Ok(()) => {
                    metrics.event_persisted();
                    persisted = true;
                    break;
                }
                Err(store_error) => {
                    metrics.write_failed(attempt);
                    warn!(
                        event_id = %event.id,
                        attempt,
                        error = %store_error,
                        "event insert failed"
                    );
                    let outcome = PersistOutcome::Transient;
                    match retry_next(attempt, outcome) {
                        RetryAction::Retry { delay_ms } => {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        }
                        RetryAction::Drop => break,
                    }
                }
            }
        }

        if !persisted {
            metrics.event_dropped(DropReason::DbError);
            error!(
                event_id = %event.id,
                "event dropped after three failed inserts"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Mutex;

    use super::*;
    use crate::ingest::EventStamp;
    use crate::port::EventPersister;
    use chrono::Utc;

    #[derive(Clone, Default)]
    struct FakePersister {
        ids: Arc<Mutex<Vec<uuid::Uuid>>>,
    }

    impl EventPersister for FakePersister {
        fn insert_event(
            &self,
            _org: OrgScope,
            event: &NewEvent,
        ) -> impl Future<Output = Result<(), gridforge_store::StoreError>> + Send {
            let ids = self.ids.clone();
            let id = event.id;
            async move {
                ids.lock().unwrap().push(id);
                Ok(())
            }
        }
    }

    #[test]
    fn transient_failures_retry_with_100ms_backoff_until_the_third_attempt() {
        assert_eq!(
            retry_next(1, PersistOutcome::Transient),
            RetryAction::Retry { delay_ms: 100 }
        );
        assert_eq!(
            retry_next(2, PersistOutcome::Transient),
            RetryAction::Retry { delay_ms: 200 }
        );
        assert_eq!(retry_next(3, PersistOutcome::Transient), RetryAction::Drop);
    }

    #[test]
    fn permanent_failures_drop_immediately() {
        assert_eq!(retry_next(1, PersistOutcome::Permanent), RetryAction::Drop);
    }

    #[tokio::test]
    async fn drains_five_events_without_a_database() {
        let persister = FakePersister::default();
        let metrics = Arc::new(Metrics::default());
        let (sender, receiver) = mpsc::channel(8);
        let org = OrgScope::new(uuid::Uuid::from_u128(1));

        for _ in 0..5 {
            let event = EventStamp::new(Utc::now(), uuid::Uuid::now_v7(), org.get()).test_event();
            sender.send(event).await.unwrap();
        }
        drop(sender);

        run(receiver, persister.clone(), metrics.clone(), org).await;

        assert_eq!(persister.ids.lock().unwrap().len(), 5);
        assert!(metrics.render().contains("events_persisted_total 5"));
    }
}
