use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewEvent {
    pub id: Uuid,
    pub device_id: String,
    pub boot_count: i64,
    pub seq: i64,
    pub schema_ver: i32,
    pub device_ts: DateTime<Utc>,
    pub ingest_ts: DateTime<Utc>,
    pub payload: Vec<u8>,
    pub payload_type: String,
}

impl NewEvent {
    pub fn test() -> Self {
        let id = Uuid::now_v7();
        let now = Utc::now();

        Self {
            id,
            device_id: "test-button".to_string(),
            boot_count: 0,
            seq: now.timestamp_millis(),
            schema_ver: 1,
            device_ts: now,
            ingest_ts: now,
            payload: format!("test-{id}").into_bytes(),
            payload_type: "text/plain".to_string(),
        }
    }

    pub fn payload_bytes(&self) -> i32 {
        self.payload.len() as i32
    }
}
