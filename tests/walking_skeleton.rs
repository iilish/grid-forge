#![cfg(feature = "db-tests")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use tower::ServiceExt;
use uuid::Uuid;

use grid_forge::http::router;
use gridforge_app::{AppContext, ChannelSink, Db, EventSink, EventStamp, Metrics, worker};
use gridforge_domain::OrgScope;

async fn test_db() -> Db {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = Db::connect(&url).await.expect("connect to Postgres");
    db.migrate().await.expect("run migrations");
    db
}

#[tokio::test]
async fn worker_persists_one_row() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let event = EventStamp::new(Utc::now(), Uuid::now_v7(), org.get()).test_event();

    db.insert_event(org, &event).await.expect("insert event");

    let rows = db.recent_events(org, 20).await.expect("query events");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, event.id);
    assert_eq!(rows[0].device_id, event.device_id);
    assert_eq!(rows[0].payload, event.payload);
}

#[tokio::test]
async fn dashboard_orders_recent_events() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let base = Utc::now();

    for index in 0..25 {
        let mut event = EventStamp::new(Utc::now(), Uuid::now_v7(), org.get()).test_event();
        event.ingest_ts = base + Duration::seconds(index);
        db.insert_event(org, &event).await.expect("insert event");
    }

    let rows = db.recent_events(org, 20).await.expect("query events");
    assert_eq!(rows.len(), 20);

    for pair in rows.windows(2) {
        assert!(pair[0].ingest_ts >= pair[1].ingest_ts);
    }
}

#[tokio::test]
async fn readyz_is_503_while_draining_and_healthz_stays_200() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let draining = Arc::new(AtomicBool::new(false));
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));

    let ctx = AppContext::new(
        db,
        sink,
        Arc::new(Metrics::default()),
        org,
        draining.clone(),
    );
    let app = router(ctx);

    let ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    draining.store(true, Ordering::SeqCst);

    let not_ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_ready.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(not_ready.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "draining");
    assert_eq!(json["draining"], true);

    let health = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn worker_retries_three_times_then_counts_failure() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let metrics = Arc::new(Metrics::default());
    let event = EventStamp::new(Utc::now(), Uuid::now_v7(), org.get()).test_event();

    db.insert_event(org, &event)
        .await
        .expect("seed duplicate id");

    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));
    sink.try_send(event).expect("queue duplicate id");
    drop(sink);

    let worker = tokio::spawn(worker::run(receiver, db.clone(), metrics.clone(), org));
    worker.await.expect("worker exits");

    let rendered = metrics.render();
    assert!(rendered.contains("events_write_failures_total{attempt=\"1\"} 1"));
    assert!(rendered.contains("events_write_failures_total{attempt=\"2\"} 1"));
    assert!(rendered.contains("events_write_failures_total{attempt=\"3\"} 1"));
    assert!(rendered.contains("events_dropped_total{reason=\"db_error\"} 1"));
    assert!(rendered.contains("events_persisted_total 0"));

    let rows = db.recent_events(org, 20).await.expect("query events");
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn worker_drains_five_events_before_exit() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let metrics = Arc::new(Metrics::default());

    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));

    for _ in 0..5 {
        sink.try_send(EventStamp::new(Utc::now(), Uuid::now_v7(), org.get()).test_event())
            .expect("queue event");
    }
    drop(sink);

    let worker = tokio::spawn(worker::run(receiver, db.clone(), metrics.clone(), org));
    worker.await.expect("worker drains");

    let rendered = metrics.render();
    assert!(rendered.contains("events_persisted_total 5"));
    assert!(rendered.contains("events_dropped_total{reason=\"channel_full\"} 0"));
    assert!(rendered.contains("events_dropped_total{reason=\"channel_closed\"} 0"));
    assert!(rendered.contains("events_dropped_total{reason=\"db_error\"} 0"));

    let rows = db.recent_events(org, 20).await.expect("query events");
    assert_eq!(rows.len(), 5);
}

#[tokio::test]
async fn duplicate_device_seq_events_are_both_stored() {
    let db = test_db().await;
    let org = OrgScope::from(Uuid::now_v7());
    let first = EventStamp::new(Utc::now(), Uuid::now_v7(), org.get()).test_event();
    let mut second = first.clone();
    second.id = Uuid::now_v7();

    db.insert_event(org, &first).await.expect("insert first");
    db.insert_event(org, &second)
        .await
        .expect("insert duplicate key");

    let rows = db.recent_events(org, 20).await.expect("query events");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|event| event.id == first.id));
    assert!(rows.iter().any(|event| event.id == second.id));
}
