use std::process::exit;
use std::time::Duration;

use chrono::{DateTime, Utc};
use gridforge_contracts::Envelope;
use prost::Message;
use prost_types::Timestamp;
use uuid::Uuid;

const DEFAULT_URL: &str = "http://localhost:8080";
const DEFAULT_ORG_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEFAULT_DEVICE_ID: &str = "simulator";

struct Args {
    url: String,
    count: u64,
    interval_ms: u64,
    org_id: String,
    device_id: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("gridforge-simulator: {error}");
        exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1))?;
    let client = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .build(),
    );
    let endpoint = format!("{}/ingest", args.url.trim_end_matches('/'));

    for index in 0..args.count {
        let now = Utc::now();
        let envelope = build_envelope(&args.org_id, &args.device_id, index as i64, now);
        let response = client
            .post(&endpoint)
            .header("content-type", "application/x-protobuf")
            .send(envelope.encode_to_vec())
            .map_err(|error| format!("POST {endpoint}: {error}"))?;

        let status = response.status().as_u16();
        if status != 202 {
            let body = response.into_body().read_to_string().unwrap_or_default();
            return Err(format!(
                "POST {endpoint}: expected 202, got {status}: {body}"
            ));
        }

        if args.interval_ms > 0 && index + 1 < args.count {
            std::thread::sleep(Duration::from_millis(args.interval_ms));
        }
    }

    println!("sent {} envelope(s) to {endpoint}", args.count);
    Ok(())
}

fn build_envelope(org_id: &str, device_id: &str, seq: i64, now: DateTime<Utc>) -> Envelope {
    Envelope {
        schema_ver: 1,
        org_id: org_id.to_string(),
        device_id: device_id.to_string(),
        boot_count: 0,
        seq,
        device_ts: Some(Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }),
        payload: format!("simulator-{}", Uuid::now_v7()).into_bytes(),
        payload_type: "text/plain".to_string(),
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        url: DEFAULT_URL.to_string(),
        count: 1,
        interval_ms: 0,
        org_id: DEFAULT_ORG_ID.to_string(),
        device_id: DEFAULT_DEVICE_ID.to_string(),
    };

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| {
            args.next()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match arg.as_str() {
            "--url" => parsed.url = value("--url")?,
            "--count" => {
                parsed.count = value("--count")?
                    .parse()
                    .map_err(|_| "--count must be a positive integer".to_string())?;
            }
            "--interval-ms" => {
                parsed.interval_ms = value("--interval-ms")?
                    .parse()
                    .map_err(|_| "--interval-ms must be an integer".to_string())?;
            }
            "--org-id" => parsed.org_id = value("--org-id")?,
            "--device-id" => parsed.device_id = value("--device-id")?,
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if parsed.count == 0 {
        return Err("--count must be greater than zero".to_string());
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Result<Args, String> {
        parse_args(words.iter().map(|word| word.to_string()))
    }

    #[test]
    fn defaults_are_stable() {
        let parsed = args(&[]).unwrap();
        assert_eq!(parsed.url, DEFAULT_URL);
        assert_eq!(parsed.count, 1);
        assert_eq!(parsed.interval_ms, 0);
        assert_eq!(parsed.org_id, DEFAULT_ORG_ID);
        assert_eq!(parsed.device_id, DEFAULT_DEVICE_ID);
    }

    #[test]
    fn parses_overrides() {
        let parsed = args(&[
            "--url",
            "http://example:9000",
            "--count",
            "3",
            "--interval-ms",
            "5",
            "--org-id",
            "00000000-0000-0000-0000-000000000002",
            "--device-id",
            "meter-1",
        ])
        .unwrap();
        assert_eq!(parsed.url, "http://example:9000");
        assert_eq!(parsed.count, 3);
        assert_eq!(parsed.interval_ms, 5);
        assert_eq!(parsed.org_id, "00000000-0000-0000-0000-000000000002");
        assert_eq!(parsed.device_id, "meter-1");
    }

    #[test]
    fn rejects_zero_count_and_unknown_flags() {
        assert!(args(&["--count", "0"]).is_err());
        assert!(args(&["--wat"]).is_err());
    }

    #[test]
    fn builds_a_decodable_envelope() {
        let envelope = build_envelope(
            "00000000-0000-0000-0000-000000000001",
            "meter-1",
            7,
            Utc::now(),
        );

        let decoded = Envelope::decode(envelope.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded.schema_ver, 1);
        assert_eq!(decoded.org_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(decoded.device_id, "meter-1");
        assert_eq!(decoded.seq, 7);
        assert_eq!(decoded.payload_type, "text/plain");
        assert!(!decoded.payload.is_empty());
        assert!(decoded.device_ts.is_some());
    }
}
