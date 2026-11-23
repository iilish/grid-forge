use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gridforge_domain::OrgScope;
use gridforge_store::Db;

use crate::metrics::Metrics;
use crate::port::EventSink;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub sink: Arc<dyn EventSink>,
    pub metrics: Arc<Metrics>,
    pub org_id: OrgScope,
    pub draining: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(
        db: Db,
        sink: Arc<dyn EventSink>,
        metrics: Arc<Metrics>,
        org_id: OrgScope,
        draining: Arc<AtomicBool>,
    ) -> Self {
        Self {
            db,
            sink,
            metrics,
            org_id,
            draining,
        }
    }
}
