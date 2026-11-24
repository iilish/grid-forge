use std::sync::Arc;
use std::time::Duration;

use gridforge_domain::{NewEvent, OrgScope};
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::metrics::Metrics;
use crate::port::EventPersister;

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

        for attempt in 1..=3 {
            match persister.insert_event(org, &event).await {
                Ok(()) => {
                    metrics.event_persisted();
                    persisted = true;
                    break;
                }
                Err(error) => {
                    metrics.write_failed(attempt);
                    warn!(
                        event_id = %event.id,
                        attempt,
                        error = %error,
                        "event insert failed"
                    );
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                    }
                }
            }
        }

        if !persisted {
            metrics.event_dropped("db_error");
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
    use crate::port::EventPersister;

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

    #[tokio::test]
    async fn drains_five_events_without_a_database() {
        let persister = FakePersister::default();
        let metrics = Arc::new(Metrics::default());
        let (sender, receiver) = mpsc::channel(8);

        for _ in 0..5 {
            sender.send(NewEvent::test()).await.unwrap();
        }
        drop(sender);

        run(
            receiver,
            persister.clone(),
            metrics.clone(),
            OrgScope::new(uuid::Uuid::from_u128(1)),
        )
        .await;

        assert_eq!(persister.ids.lock().unwrap().len(), 5);
        assert!(metrics.render().contains("events_persisted_total 5"));
    }
}
