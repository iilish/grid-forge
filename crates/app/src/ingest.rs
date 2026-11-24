use chrono::{DateTime, Utc};
use gridforge_contracts::Envelope;
use gridforge_domain::NewEvent;
use gridforge_store::{RecentEvent, StoreError};
use prost_types::Timestamp;
use uuid::Uuid;

use crate::error::AppError;
use crate::metrics::Route;
use crate::state::AppState;

const MAX_DEVICE_ID_CHARS: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

pub fn create_ingest_event(state: &AppState, envelope: &Envelope) -> Result<Uuid, AppError> {
    enqueue(state, envelope, Route::Ingest)
}

pub fn create_test_event(state: &AppState) -> Result<Uuid, AppError> {
    let now = Utc::now();
    let envelope = Envelope {
        schema_ver: 1,
        org_id: state.org_id.to_string(),
        device_id: "test-button".to_string(),
        boot_count: 0,
        seq: now.timestamp_millis(),
        device_ts: Some(Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }),
        payload: format!("test-{}", Uuid::now_v7()).into_bytes(),
        payload_type: "text/plain".to_string(),
    };

    enqueue(state, &envelope, Route::Test)
}

fn enqueue(state: &AppState, envelope: &Envelope, route: Route) -> Result<Uuid, AppError> {
    validate(state.org_id.get(), envelope)?;

    let event = NewEvent {
        id: Uuid::now_v7(),
        device_id: envelope.device_id.clone(),
        boot_count: envelope.boot_count,
        seq: envelope.seq,
        schema_ver: envelope.schema_ver as i32,
        device_ts: device_ts(envelope)?,
        ingest_ts: Utc::now(),
        payload: envelope.payload.clone(),
        payload_type: envelope.payload_type.clone(),
    };
    let id = event.id;

    match state.sink.try_send(event) {
        Ok(()) => {
            state.metrics.event_enqueued(route);
            Ok(id)
        }
        Err(error) => {
            state.metrics.event_dropped(error.reason());
            Err(AppError::from(error))
        }
    }
}

fn validate(org_id: Uuid, envelope: &Envelope) -> Result<(), AppError> {
    if envelope.schema_ver != 1 {
        return Err(invalid("schema_ver must be 1"));
    }

    let envelope_org_id =
        Uuid::parse_str(&envelope.org_id).map_err(|_| invalid("org_id must be a UUID string"))?;
    if envelope_org_id != org_id {
        return Err(invalid("org_id must match the configured org"));
    }

    let device_len = envelope.device_id.chars().count();
    let device_chars_valid = envelope.device_id.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
    });
    if device_len == 0 || device_len > MAX_DEVICE_ID_CHARS || !device_chars_valid {
        return Err(invalid(
            "device_id must be 1..128 characters of [A-Za-z0-9._:-]",
        ));
    }

    if envelope.boot_count < 0 {
        return Err(invalid("boot_count must be >= 0"));
    }

    if envelope.seq < 0 {
        return Err(invalid("seq must be >= 0"));
    }

    if envelope.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(invalid("payload must be at most 32 KiB"));
    }

    if envelope.payload_type.is_empty() {
        return Err(invalid("payload_type must not be empty"));
    }

    Ok(())
}

fn device_ts(envelope: &Envelope) -> Result<DateTime<Utc>, AppError> {
    match envelope.device_ts {
        Some(timestamp) => DateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
            .ok_or_else(|| invalid("device_ts must be a valid timestamp")),
        None => Ok(Utc::now()),
    }
}

fn invalid(detail: &str) -> AppError {
    AppError::Validation(detail.to_string())
}

pub async fn recent_events(state: &AppState, limit: i64) -> Result<Vec<RecentEvent>, StoreError> {
    state.db.recent_events(state.org_id, limit).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::metrics::{Metrics, Route};
    use crate::port::ChannelSink;
    use gridforge_domain::OrgScope;

    const ORG: Uuid = Uuid::from_u128(1);

    async fn state() -> (AppState, tokio::sync::mpsc::Receiver<NewEvent>) {
        let db = gridforge_store::Db::lazy("postgres://127.0.0.1:1/gridforge")
            .await
            .unwrap();

        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        let state = AppState::new(
            db,
            Arc::new(ChannelSink::new(sender)),
            Arc::new(Metrics::default()),
            OrgScope::new(ORG),
            Arc::new(AtomicBool::new(false)),
        );
        (state, receiver)
    }

    fn valid_envelope() -> Envelope {
        Envelope {
            schema_ver: 1,
            org_id: ORG.to_string(),
            device_id: "device-1".to_string(),
            boot_count: 0,
            seq: 1,
            device_ts: Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            payload: b"hello".to_vec(),
            payload_type: "text/plain".to_string(),
        }
    }

    fn assert_invalid(envelope: Envelope, detail: &str) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (state, _receiver) = state().await;
            let error = create_ingest_event(&state, &envelope).unwrap_err();
            assert!(
                matches!(error, AppError::Validation(_)),
                "expected validation error, got {error:?}"
            );
            assert!(error.detail().contains(detail), "{}", error.detail());
        });
    }

    #[test]
    fn accepts_a_valid_envelope() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (state, _receiver) = state().await;
            let id = create_ingest_event(&state, &valid_envelope()).unwrap();
            assert_eq!(id.get_version_num(), 7);
            assert!(
                state
                    .metrics
                    .render()
                    .contains("events_received_total{route=\"ingest\"} 1")
            );
        });
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut envelope = valid_envelope();
        envelope.schema_ver = 2;
        assert_invalid(envelope, "schema_ver");
    }

    #[test]
    fn rejects_org_id_that_is_not_a_uuid() {
        let mut envelope = valid_envelope();
        envelope.org_id = "not-a-uuid".to_string();
        assert_invalid(envelope, "org_id");
    }

    #[test]
    fn rejects_org_id_mismatch() {
        let mut envelope = valid_envelope();
        envelope.org_id = Uuid::from_u128(2).to_string();
        assert_invalid(envelope, "org_id");
    }

    #[test]
    fn rejects_empty_device_id() {
        let mut envelope = valid_envelope();
        envelope.device_id = String::new();
        assert_invalid(envelope, "device_id");
    }

    #[test]
    fn rejects_device_id_with_illegal_characters() {
        let mut envelope = valid_envelope();
        envelope.device_id = "device id".to_string();
        assert_invalid(envelope, "device_id");
    }

    #[test]
    fn rejects_device_id_longer_than_128_characters() {
        let mut envelope = valid_envelope();
        envelope.device_id = "a".repeat(129);
        assert_invalid(envelope, "device_id");
    }

    #[test]
    fn rejects_negative_boot_count() {
        let mut envelope = valid_envelope();
        envelope.boot_count = -1;
        assert_invalid(envelope, "boot_count");
    }

    #[test]
    fn rejects_negative_seq() {
        let mut envelope = valid_envelope();
        envelope.seq = -1;
        assert_invalid(envelope, "seq");
    }

    #[test]
    fn accepts_payload_at_limit_and_rejects_one_byte_over() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (state, _receiver) = state().await;
            let mut envelope = valid_envelope();
            envelope.payload = vec![0; MAX_PAYLOAD_BYTES];
            assert!(create_ingest_event(&state, &envelope).is_ok());

            envelope.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
            let error = create_ingest_event(&state, &envelope).unwrap_err();
            assert!(error.detail().contains("payload"));
        });
    }

    #[test]
    fn rejects_empty_payload_type() {
        let mut envelope = valid_envelope();
        envelope.payload_type = String::new();
        assert_invalid(envelope, "payload_type");
    }

    #[test]
    fn test_event_uses_test_route() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (state, _receiver) = state().await;
            create_test_event(&state).unwrap();
            assert!(
                state
                    .metrics
                    .render()
                    .contains("events_received_total{route=\"test\"} 1")
            );
        });
    }

    #[test]
    fn route_labels_are_stable() {
        assert_eq!(Route::Test.label(), "test");
        assert_eq!(Route::Ingest.label(), "ingest");
    }
}
