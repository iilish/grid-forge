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
    pub fn payload_bytes(&self) -> i32 {
        self.payload.len() as i32
    }
}

pub fn resolve_device_ts(device_ts: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    device_ts.unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_now_when_device_ts_is_missing() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        assert_eq!(resolve_device_ts(None, now), now);
    }

    #[test]
    fn keeps_device_ts_when_present() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let device = DateTime::from_timestamp(1_600_000_000, 0).unwrap();
        assert_eq!(resolve_device_ts(Some(device), now), device);
    }
}
