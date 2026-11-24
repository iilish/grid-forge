use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::state::AppContext;

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

pub struct ReadinessReport {
    pub db_up: bool,
    pub draining: bool,
}

pub async fn readiness(ctx: &AppContext) -> ReadinessReport {
    let draining = ctx.health.draining.load(Ordering::SeqCst);
    let db_up = tokio::time::timeout(PROBE_TIMEOUT, ctx.health.db.probe())
        .await
        .unwrap_or(false);

    ReadinessReport { db_up, draining }
}
