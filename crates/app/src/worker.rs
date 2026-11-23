use std::sync::Arc;
use std::time::Duration;

use gridforge_domain::{NewEvent, OrgScope};
use gridforge_store::Db;
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::metrics::Metrics;

pub async fn run(
    mut receiver: mpsc::Receiver<NewEvent>,
    db: Db,
    metrics: Arc<Metrics>,
    org: OrgScope,
) {
    while let Some(event) = receiver.recv().await {
        let mut persisted = false;

        for attempt in 1..=3 {
            match db.insert_event(org, &event).await {
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
