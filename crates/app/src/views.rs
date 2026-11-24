use gridforge_store::{RecentEvent, StoreError};

use crate::state::QueryCtx;

pub async fn recent_events(query: &QueryCtx, limit: i64) -> Result<Vec<RecentEvent>, StoreError> {
    query.db.recent_events(query.org, limit).await
}
