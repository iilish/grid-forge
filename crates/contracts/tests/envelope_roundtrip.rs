use gridforge_contracts::Envelope;
use prost::Message;
use prost_types::Timestamp;

#[test]
fn envelope_round_trips_timestamp_and_payload() {
    let envelope = Envelope {
        schema_ver: 1,
        org_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        device_id: "device-1".to_owned(),
        boot_count: 3,
        seq: 42,
        device_ts: Some(Timestamp {
            seconds: 1_700_000_000,
            nanos: 123_000_000,
        }),
        payload: br#"{"kw":1.5}"#.to_vec(),
        payload_type: "application/json".to_owned(),
    };

    let encoded = envelope.encode_to_vec();
    let decoded = Envelope::decode(encoded.as_slice()).expect("valid envelope");

    assert_eq!(decoded, envelope);
}

#[test]
fn malformed_tag_is_rejected() {
    assert!(Envelope::decode([0xFF].as_slice()).is_err());
}
