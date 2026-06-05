use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use westpunkte_metrics::client::WestbahnClient;
use westpunkte_metrics::config::Config;
use westpunkte_metrics::metrics::MetricsSnapshot;
use westpunkte_metrics::server::{self, SharedSnapshot};

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--healthcheck") {
        return healthcheck().await;
    }

    init_tracing();

    let config = Config::from_env().context("Failed to load configuration from environment")?;
    info!(
        port = config.port,
        refresh_secs = config.refresh_interval.as_secs(),
        base_url = %config.base_url,
        "starting westpunkte-metrics exporter"
    );

    let snapshot: SharedSnapshot = Arc::new(RwLock::new(MetricsSnapshot::default()));
    let client = WestbahnClient::new(
        &config.base_url,
        &config.auth_token,
        &config.auth_id_email,
        config.request_timeout,
    )?;

    let refresh_state = snapshot.clone();
    let refresh_interval = config.refresh_interval;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(refresh_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            refresh(&client, &refresh_state).await;
        }
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;
    info!(%addr, "listening");

    axum::serve(listener, server::router(snapshot))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server failed")?;

    info!("shutdown complete");
    Ok(())
}

async fn refresh(client: &WestbahnClient, state: &SharedSnapshot) {
    let now = Utc::now();
    let result = client.fetch_points().await;
    let mut snap = state.write().await;
    snap.last_attempt = Some(now);
    match result {
        Ok(points) => {
            info!(balance = points.total, "fetched points");
            snap.last_success = Some(now);
            snap.last_error = None;
            snap.points = Some(points);
        }
        Err(e) => {
            error!(error = %format!("{e:#}"), "failed to fetch points");
            snap.last_error = Some(format!("{e:#}"));
        }
    }
}

async fn healthcheck() -> Result<()> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9090);
    let url = format!("http://127.0.0.1:{port}/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("Failed to build healthcheck HTTP client")?;
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Healthcheck request to {url} failed"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("Healthcheck got HTTP {}", resp.status());
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("westpunkte_metrics=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("received Ctrl+C"),
        _ = terminate => info!("received SIGTERM"),
    }
}
