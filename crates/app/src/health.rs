use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::state::AppState;

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

pub struct Readiness {
    pub db_up: bool,
    pub draining: bool,
}

pub async fn readiness(state: &AppState) -> Readiness {
    let draining = state.draining.load(Ordering::SeqCst);
    let db_up = tokio::time::timeout(PROBE_TIMEOUT, state.db.probe())
        .await
        .unwrap_or(false);

    Readiness { db_up, draining }
}
