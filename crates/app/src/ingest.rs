use gridforge_domain::NewEvent;
use gridforge_store::{RecentEvent, StoreError};
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

pub fn create_test_event(state: &AppState) -> Result<Uuid, AppError> {
    let event = NewEvent::test();
    let id = event.id;

    match state.sink.try_send(event) {
        Ok(()) => {
            state.metrics.event_enqueued();
            Ok(id)
        }
        Err(error) => {
            state.metrics.event_dropped(error.reason());
            Err(AppError::from(error))
        }
    }
}

pub async fn recent_events(state: &AppState, limit: i64) -> Result<Vec<RecentEvent>, StoreError> {
    state.db.recent_events(state.org_id, limit).await
}
