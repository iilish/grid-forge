use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const BUCKETS: [f64; 9] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5];
const BUCKET_COUNT: usize = BUCKETS.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Test,
    Ingest,
}

impl Route {
    pub fn label(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Ingest => "ingest",
        }
    }
}

#[derive(Default)]
pub struct Metrics {
    events_received_test: AtomicU64,
    events_received_ingest: AtomicU64,
    events_persisted: AtomicU64,
    dropped_channel_full: AtomicU64,
    dropped_channel_closed: AtomicU64,
    dropped_db_error: AtomicU64,
    write_failure_1: AtomicU64,
    write_failure_2: AtomicU64,
    write_failure_3: AtomicU64,
    http_requests: AtomicU64,
    http_durations: Mutex<BTreeMap<HttpKey, Histogram>>,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct HttpKey {
    method: String,
    route: String,
    status_class: String,
}

#[derive(Default)]
struct Histogram {
    count: u64,
    sum_micros: u64,
    buckets: [u64; BUCKET_COUNT],
}

impl Metrics {
    pub fn event_enqueued(&self, route: Route) {
        let counter = match route {
            Route::Test => &self.events_received_test,
            Route::Ingest => &self.events_received_ingest,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn event_persisted(&self) {
        self.events_persisted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn event_dropped(&self, reason: &str) {
        let counter = match reason {
            "channel_full" => &self.dropped_channel_full,
            "channel_closed" => &self.dropped_channel_closed,
            "db_error" => &self.dropped_db_error,
            _ => return,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn write_failed(&self, attempt: usize) {
        let counter = match attempt {
            1 => &self.write_failure_1,
            2 => &self.write_failure_2,
            3 => &self.write_failure_3,
            _ => return,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_http(&self, method: &str, route: &str, status_class: &str, elapsed: Duration) {
        self.http_requests.fetch_add(1, Ordering::Relaxed);

        let key = HttpKey {
            method: method.to_string(),
            route: route.to_string(),
            status_class: status_class.to_string(),
        };
        let mut durations = self
            .http_durations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let histogram = durations.entry(key).or_default();
        histogram.count += 1;
        histogram.sum_micros += elapsed.as_micros() as u64;

        let seconds = elapsed.as_secs_f64();
        for (index, bucket) in BUCKETS.iter().enumerate() {
            if seconds <= *bucket {
                histogram.buckets[index] += 1;
            }
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();

        counter(
            &mut out,
            "events_received_total",
            "Events accepted by the sink.",
            &[("route", Route::Test.label())],
            &self.events_received_test,
        );
        counter(
            &mut out,
            "events_received_total",
            "Events accepted by the sink.",
            &[("route", Route::Ingest.label())],
            &self.events_received_ingest,
        );
        counter(
            &mut out,
            "events_persisted_total",
            "Events committed to Postgres.",
            &[],
            &self.events_persisted,
        );
        counter(
            &mut out,
            "events_dropped_total",
            "Events that will never be persisted.",
            &[("reason", "channel_full")],
            &self.dropped_channel_full,
        );
        counter(
            &mut out,
            "events_dropped_total",
            "Events that will never be persisted.",
            &[("reason", "channel_closed")],
            &self.dropped_channel_closed,
        );
        counter(
            &mut out,
            "events_dropped_total",
            "Events that will never be persisted.",
            &[("reason", "db_error")],
            &self.dropped_db_error,
        );
        counter(
            &mut out,
            "events_write_failures_total",
            "Failed database write attempts.",
            &[("attempt", "1")],
            &self.write_failure_1,
        );
        counter(
            &mut out,
            "events_write_failures_total",
            "Failed database write attempts.",
            &[("attempt", "2")],
            &self.write_failure_2,
        );
        counter(
            &mut out,
            "events_write_failures_total",
            "Failed database write attempts.",
            &[("attempt", "3")],
            &self.write_failure_3,
        );
        counter(
            &mut out,
            "http_requests_total",
            "HTTP requests observed.",
            &[],
            &self.http_requests,
        );

        let _ = writeln!(
            out,
            "# HELP http_request_duration_seconds HTTP request duration."
        );
        let _ = writeln!(out, "# TYPE http_request_duration_seconds histogram");
        let durations = self
            .http_durations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (key, histogram) in durations.iter() {
            for (index, bucket) in BUCKETS.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "http_request_duration_seconds_bucket{{method=\"{}\",route=\"{}\",status_class=\"{}\",le=\"{}\"}} {}",
                    key.method, key.route, key.status_class, bucket, histogram.buckets[index]
                );
            }
            let _ = writeln!(
                out,
                "http_request_duration_seconds_bucket{{method=\"{}\",route=\"{}\",status_class=\"{}\",le=\"+Inf\"}} {}",
                key.method, key.route, key.status_class, histogram.count
            );
            let _ = writeln!(
                out,
                "http_request_duration_seconds_count{{method=\"{}\",route=\"{}\",status_class=\"{}\"}} {}",
                key.method, key.route, key.status_class, histogram.count
            );
            let _ = writeln!(
                out,
                "http_request_duration_seconds_sum{{method=\"{}\",route=\"{}\",status_class=\"{}\"}} {:.6}",
                key.method,
                key.route,
                key.status_class,
                histogram.sum_micros as f64 / 1_000_000.0
            );
        }

        out
    }
}

fn counter(out: &mut String, name: &str, help: &str, labels: &[(&str, &str)], value: &AtomicU64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    let labels = match labels {
        [] => String::new(),
        labels => {
            let inner = labels
                .iter()
                .map(|(key, value)| format!("{key}=\"{value}\""))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
    };
    let _ = writeln!(out, "{name}{labels} {}", value.load(Ordering::Relaxed));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_counters_and_histogram() {
        let metrics = Metrics::default();
        metrics.event_enqueued(Route::Test);
        metrics.event_enqueued(Route::Ingest);
        metrics.event_dropped("channel_full");
        metrics.write_failed(2);
        metrics.observe_http("GET", "/", "2xx", Duration::from_millis(20));

        let rendered = metrics.render();

        assert!(rendered.contains("events_received_total{route=\"test\"} 1"));
        assert!(rendered.contains("events_received_total{route=\"ingest\"} 1"));
        assert!(rendered.contains("events_dropped_total{reason=\"channel_full\"} 1"));
        assert!(rendered.contains("events_write_failures_total{attempt=\"2\"} 1"));
        assert!(rendered.contains(
            "http_request_duration_seconds_count{method=\"GET\",route=\"/\",status_class=\"2xx\"} 1"
        ));
        assert!(rendered.contains(
            "http_request_duration_seconds_bucket{method=\"GET\",route=\"/\",status_class=\"2xx\",le=\"0.025\"} 1"
        ));
    }
}
