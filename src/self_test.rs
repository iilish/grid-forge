use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, ensure};
use tokio::sync::mpsc;
use uuid::Uuid;

use gridforge_app::{
    AppError, AppState, ChannelSink, Db, EventPersister, EventSink, Metrics, SinkError,
    create_test_event, readiness, worker,
};
use gridforge_domain::{NewEvent, OrgScope};

const UNREACHABLE_DATABASE: &str = "postgres://127.0.0.1:1/gridforge";

pub async fn run(name: Option<String>) -> Result<()> {
    let Some(name) = name else {
        return Err(anyhow!(
            "usage: gridforge self-test channel-full|db-retry|drain"
        ));
    };

    let result = match name.as_str() {
        "channel-full" => channel_full().await,
        "db-retry" => db_retry().await,
        "drain" => drain().await,
        other => Err(anyhow!("unknown self-test: {other}")),
    };

    match &result {
        Ok(()) => println!("PASS self-test {name}"),
        Err(error) => eprintln!("FAIL self-test {name}: {error:#}"),
    }

    result
}

async fn channel_full() -> Result<()> {
    let (state, _receiver) = state_with_capacity(1).await?;

    create_test_event(&state).map_err(|error| anyhow!("first send failed: {error}"))?;
    let error = create_test_event(&state).expect_err("second send must fail on a full channel");
    ensure!(
        matches!(error, AppError::Sink(SinkError::Full)),
        "second send returned {error:?}, expected sink full"
    );

    let rendered = state.metrics.render();
    ensure!(
        rendered.contains("events_dropped_total{reason=\"channel_full\"} 1"),
        "channel_full drop was not counted:\n{rendered}"
    );
    ensure!(
        rendered.contains("events_received_total{route=\"test\"} 1"),
        "accepted test event was not counted:\n{rendered}"
    );

    Ok(())
}

async fn db_retry() -> Result<()> {
    let db = Db::lazy(UNREACHABLE_DATABASE)
        .await
        .map_err(|error| anyhow!("create lazy pool: {error}"))?;
    let metrics = Arc::new(Metrics::default());
    let org = sentinel_org();

    let (sender, receiver) = mpsc::channel(1);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));
    sink.try_send(NewEvent::test())
        .map_err(|error| anyhow!("queue event: {error}"))?;
    drop(sink);

    worker::run(receiver, db, metrics.clone(), org).await;

    let rendered = metrics.render();
    for attempt in 1..=3 {
        ensure!(
            rendered.contains(&format!(
                "events_write_failures_total{{attempt=\"{attempt}\"}} 1"
            )),
            "write attempt {attempt} was not counted:\n{rendered}"
        );
    }
    ensure!(
        rendered.contains("events_dropped_total{reason=\"db_error\"} 1"),
        "db_error drop was not counted:\n{rendered}"
    );
    ensure!(
        rendered.contains("events_persisted_total 0"),
        "unreachable database reported a persisted event:\n{rendered}"
    );

    Ok(())
}

async fn drain() -> Result<()> {
    let persister = FakePersister::default();
    let metrics = Arc::new(Metrics::default());
    let draining = Arc::new(AtomicBool::new(false));
    let org = sentinel_org();
    let db = Db::lazy(UNREACHABLE_DATABASE)
        .await
        .map_err(|error| anyhow!("create lazy pool: {error}"))?;

    let (sender, receiver) = mpsc::channel(8);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));
    let state = AppState::new(db, sink.clone(), metrics.clone(), org, draining.clone());

    for _ in 0..5 {
        sink.try_send(NewEvent::test())
            .map_err(|error| anyhow!("queue event: {error}"))?;
    }

    draining.store(true, Ordering::SeqCst);
    let report = readiness(&state).await;
    ensure!(report.draining, "readiness did not report draining");
    ensure!(
        !(report.db_up && !report.draining),
        "readiness reported ready while draining"
    );

    let worker = tokio::spawn(worker::run(
        receiver,
        persister.clone(),
        metrics.clone(),
        org,
    ));
    drop(state);
    drop(sink);
    worker
        .await
        .map_err(|error| anyhow!("worker task failed: {error}"))?;

    ensure!(
        persister.ids.lock().unwrap().len() == 5,
        "worker persisted {} events, expected 5",
        persister.ids.lock().unwrap().len()
    );
    ensure!(
        metrics.render().contains("events_persisted_total 5"),
        "persisted counter does not show five events:\n{}",
        metrics.render()
    );

    Ok(())
}

async fn state_with_capacity(capacity: usize) -> Result<(AppState, mpsc::Receiver<NewEvent>)> {
    let db = Db::lazy(UNREACHABLE_DATABASE)
        .await
        .map_err(|error| anyhow!("create lazy pool: {error}"))?;
    let (sender, receiver) = mpsc::channel(capacity);
    let state = AppState::new(
        db,
        Arc::new(ChannelSink::new(sender)),
        Arc::new(Metrics::default()),
        sentinel_org(),
        Arc::new(AtomicBool::new(false)),
    );

    Ok((state, receiver))
}

fn sentinel_org() -> OrgScope {
    OrgScope::new(Uuid::from_u128(1))
}

#[derive(Clone, Default)]
struct FakePersister {
    ids: Arc<Mutex<Vec<Uuid>>>,
}

impl EventPersister for FakePersister {
    fn insert_event(
        &self,
        _org: OrgScope,
        event: &NewEvent,
    ) -> impl Future<Output = Result<(), gridforge_app::StoreError>> + Send {
        let ids = self.ids.clone();
        let id = event.id;
        async move {
            ids.lock().unwrap().push(id);
            Ok(())
        }
    }
}
