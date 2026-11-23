use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use grid_forge::config::Config;
use grid_forge::http;
use gridforge_app::worker;
use gridforge_app::{AppState, ChannelSink, Db, EventSink, Metrics};
use gridforge_domain::OrgScope;

const CHANNEL_CAPACITY: usize = 256;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "serve".to_string());

    match command.as_str() {
        "serve" => serve().await,
        "migrate" => migrate().await,
        other => {
            eprintln!("usage: gridforge [serve|migrate]");
            anyhow::bail!("unknown command: {other}");
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,gridforge=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}

async fn migrate() -> Result<()> {
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db = Db::connect(&database_url)
        .await
        .context("connect to Postgres")?;

    db.migrate().await.context("run migrations")?;
    info!("migrations complete");
    db.close().await;
    Ok(())
}

async fn serve() -> Result<()> {
    let config = Config::from_env()?;
    let db = Db::lazy(&config.database_url)
        .await
        .context("create Postgres pool")?;

    let metrics = Arc::new(Metrics::default());
    let draining = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
    let sink: Arc<dyn EventSink> = Arc::new(ChannelSink::new(sender));

    let org = OrgScope::from(config.org_id);
    let worker = tokio::spawn(worker::run(receiver, db.clone(), metrics.clone(), org));

    let state = AppState::new(db.clone(), sink, metrics, org, draining.clone());
    let app = http::router(state.clone());
    let listener = TcpListener::bind(config.http_addr)
        .await
        .with_context(|| format!("bind {}", config.http_addr))?;

    info!(
        address = %listener.local_addr()?,
        build_sha = grid_forge::BUILD_SHA,
        "gridforge listening"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(draining))
        .await
        .context("serve HTTP")?;

    drop(state);

    match tokio::time::timeout(DRAIN_TIMEOUT, worker).await {
        Ok(Ok(())) => info!("worker drained"),
        Ok(Err(error)) => error!(error = %error, "worker task failed"),
        Err(_) => error!("worker did not drain within 5 seconds"),
    }

    db.close().await;
    info!("gridforge stopped");
    Ok(())
}

async fn shutdown_signal(draining: Arc<AtomicBool>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    draining.store(true, std::sync::atomic::Ordering::SeqCst);
    info!("shutdown requested");
}
