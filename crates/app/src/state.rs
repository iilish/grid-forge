use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gridforge_domain::OrgScope;
use gridforge_store::Db;

use crate::metrics::Metrics;
use crate::port::EventSink;

#[derive(Clone)]
pub struct IngestCtx {
    pub sink: Arc<dyn EventSink>,
    pub metrics: Arc<Metrics>,
    pub org: OrgScope,
}

#[derive(Clone)]
pub struct HealthCtx {
    pub db: Db,
    pub draining: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct QueryCtx {
    pub db: Db,
    pub org: OrgScope,
}

#[derive(Clone)]
pub struct AppContext {
    pub ingest: IngestCtx,
    pub health: HealthCtx,
    pub query: QueryCtx,
}

impl AppContext {
    pub fn new(
        db: Db,
        sink: Arc<dyn EventSink>,
        metrics: Arc<Metrics>,
        org_id: OrgScope,
        draining: Arc<AtomicBool>,
    ) -> Self {
        Self {
            ingest: IngestCtx {
                sink,
                metrics,
                org: org_id,
            },
            health: HealthCtx {
                db: db.clone(),
                draining,
            },
            query: QueryCtx { db, org: org_id },
        }
    }
}
