use chrono::{DateTime, Utc};
use gridforge_contracts::Envelope;
use gridforge_domain::{NewEvent, resolve_device_ts};
use prost_types::Timestamp;
use uuid::Uuid;

use crate::error::{AppError, ValidationError};
use crate::metrics::{DropReason, Route};
use crate::port::SinkError;
use crate::state::IngestCtx;

const MAX_DEVICE_ID_CHARS: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

pub fn create_ingest_event(ingest: &IngestCtx, envelope: &Envelope) -> Result<Uuid, AppError> {
    enqueue(ingest, envelope, Route::Ingest)
}

pub fn create_test_event(ingest: &IngestCtx) -> Result<Uuid, AppError> {
    let stamp = EventStamp::new(Utc::now(), Uuid::now_v7(), ingest.org.get());
    let envelope = stamp.test_envelope();

    enqueue(ingest, &envelope, Route::Test)
}

pub struct EventStamp {
    pub now: DateTime<Utc>,
    pub id: Uuid,
    pub org: Uuid,
}

impl EventStamp {
    pub fn new(now: DateTime<Utc>, id: Uuid, org: Uuid) -> Self {
        Self { now, id, org }
    }

    pub fn test_envelope(&self) -> Envelope {
        Envelope {
            schema_ver: 1,
            org_id: self.org.to_string(),
            device_id: "test-button".to_string(),
            boot_count: 0,
            seq: self.now.timestamp_millis(),
            device_ts: Some(Timestamp {
                seconds: self.now.timestamp(),
                nanos: self.now.timestamp_subsec_nanos() as i32,
            }),
            payload: format!("test-{}", self.id).into_bytes(),
            payload_type: "text/plain".to_string(),
        }
    }

    pub fn test_event(&self) -> NewEvent {
        self.test_envelope()
            .to_event(self)
            .expect("test envelope is valid")
    }
}

pub trait EnvelopeExt {
    fn to_event(&self, stamp: &EventStamp) -> Result<NewEvent, ValidationError>;
}

impl EnvelopeExt for Envelope {
    fn to_event(&self, stamp: &EventStamp) -> Result<NewEvent, ValidationError> {
        validate(stamp.org, self)?;

        Ok(NewEvent {
            id: stamp.id,
            device_id: self.device_id.clone(),
            boot_count: self.boot_count,
            seq: self.seq,
            schema_ver: self.schema_ver as i32,
            device_ts: resolve_device_ts(device_ts(self)?, stamp.now),
            ingest_ts: stamp.now,
            payload: self.payload.clone(),
            payload_type: self.payload_type.clone(),
        })
    }
}

fn enqueue(ingest: &IngestCtx, envelope: &Envelope, route: Route) -> Result<Uuid, AppError> {
    let stamp = EventStamp::new(Utc::now(), Uuid::now_v7(), ingest.org.get());
    let event = envelope.to_event(&stamp)?;
    let id = event.id;

    match ingest.sink.try_send(event) {
        Ok(()) => {
            ingest.metrics.event_enqueued(route);
            Ok(id)
        }
        Err(error) => {
            let reason = match error {
                SinkError::Full => DropReason::ChannelFull,
                SinkError::Closed => DropReason::ChannelClosed,
            };
            ingest.metrics.event_dropped(reason);
            Err(AppError::from(error))
        }
    }
}

fn validate(org: Uuid, envelope: &Envelope) -> Result<(), ValidationError> {
    if envelope.schema_ver != 1 {
        return Err(invalid("schema_ver must be 1"));
    }

    let envelope_org_id =
        Uuid::parse_str(&envelope.org_id).map_err(|_| invalid("org_id must be a UUID string"))?;
    if envelope_org_id != org {
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

fn device_ts(envelope: &Envelope) -> Result<Option<DateTime<Utc>>, ValidationError> {
    match envelope.device_ts {
        Some(timestamp) => DateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
            .map(Some)
            .ok_or_else(|| invalid("device_ts must be a valid timestamp")),
        None => Ok(None),
    }
}

fn invalid(detail: &str) -> ValidationError {
    ValidationError {
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::metrics::{Metrics, Route};
    use crate::port::ChannelSink;
    use crate::state::AppContext;
    use gridforge_domain::OrgScope;

    const ORG: Uuid = Uuid::from_u128(1);

    async fn state() -> (AppContext, tokio::sync::mpsc::Receiver<NewEvent>) {
        let db = gridforge_store::Db::lazy("postgres://127.0.0.1:1/gridforge")
            .await
            .unwrap();

        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        let ctx = AppContext::new(
            db,
            Arc::new(ChannelSink::new(sender)),
            Arc::new(Metrics::default()),
            OrgScope::new(ORG),
            Arc::new(AtomicBool::new(false)),
        );
        (ctx, receiver)
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
            let (ctx, _receiver) = state().await;
            let error = create_ingest_event(&ctx.ingest, &envelope).unwrap_err();
            assert!(
                matches!(error, AppError::Validation(_)),
                "expected validation error, got {error:?}"
            );
            assert!(error.detail().contains(detail), "{}", error.detail());
        });
    }

    #[test]
    fn build_event_uses_now_when_device_ts_is_missing() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut envelope = valid_envelope();
        envelope.device_ts = None;

        let event = envelope
            .to_event(&EventStamp::new(now, Uuid::from_u128(7), ORG))
            .unwrap();

        assert_eq!(event.device_ts, now);
        assert_eq!(event.ingest_ts, now);
    }

    #[test]
    fn build_event_rejects_invalid_device_ts() {
        let mut envelope = valid_envelope();
        envelope.device_ts = Some(Timestamp {
            seconds: i64::MAX,
            nanos: 0,
        });

        let error = envelope
            .to_event(&EventStamp::new(Utc::now(), Uuid::from_u128(7), ORG))
            .unwrap_err();
        assert_eq!(error.detail, "device_ts must be a valid timestamp");
    }

    #[test]
    fn test_event_is_valid_and_carries_the_id_in_the_payload() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let id = Uuid::from_u128(7);

        let event = EventStamp::new(now, id, ORG).test_event();

        assert_eq!(event.id, id);
        assert_eq!(event.payload, format!("test-{id}").into_bytes());
        assert_eq!(event.device_id, "test-button");
    }

    #[test]
    fn accepts_a_valid_envelope() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (ctx, _receiver) = state().await;
            let id = create_ingest_event(&ctx.ingest, &valid_envelope()).unwrap();
            assert_eq!(id.get_version_num(), 7);
            assert!(
                ctx.ingest
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
            let (ctx, _receiver) = state().await;
            let mut envelope = valid_envelope();
            envelope.payload = vec![0; MAX_PAYLOAD_BYTES];
            assert!(create_ingest_event(&ctx.ingest, &envelope).is_ok());

            envelope.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
            let error = create_ingest_event(&ctx.ingest, &envelope).unwrap_err();
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
            let (ctx, _receiver) = state().await;
            create_test_event(&ctx.ingest).unwrap();
            assert!(
                ctx.ingest
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
