use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use serde_json::json;
use tokio::sync::RwLock;
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use westpunkte_metrics::client::{ExpiryInfo, WestbahnClient};
use westpunkte_metrics::metrics::MetricsSnapshot;
use westpunkte_metrics::server::{self, SharedSnapshot};

fn client_for(mock: &MockServer) -> WestbahnClient {
    WestbahnClient::new(
        mock.uri(),
        "test-token",
        "1:user@example.com",
        Duration::from_secs(5),
    )
    .expect("client builds")
}

#[tokio::test]
async fn client_parses_balance_with_expiry() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/customer"))
        .and(header_regex(
            "cookie",
            r"customer_auth_token=test-token.*customer_auth_id_email=1:user@example\.com",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "customer": {
                "westpunkte": 182,
                "westpunkte_expiry": { "date": "2026-06-12T21:59:59", "amount": 16 }
            },
            "message": "ok"
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let data = client_for(&mock).fetch_points().await.unwrap();
    assert_eq!(data.total, 182);
    assert_eq!(
        data.expiry,
        Some(ExpiryInfo {
            date: NaiveDate::from_ymd_opt(2026, 6, 12).unwrap(),
            amount: 16
        })
    );
}

#[tokio::test]
async fn client_treats_session_expiry_as_error() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/customer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": false,
            "message": "Not authenticated."
        })))
        .mount(&mock)
        .await;

    let err = client_for(&mock).fetch_points().await.unwrap_err();
    assert!(format!("{err:#}").contains("Not authenticated"));
}

#[tokio::test]
async fn client_surfaces_http_error_status() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/customer"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
        .mount(&mock)
        .await;

    let err = client_for(&mock).fetch_points().await.unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("503"), "got: {msg}");
}

#[tokio::test]
async fn metrics_endpoint_serves_prometheus_format() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/customer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "customer": {
                "westpunkte": 221,
                "westpunkte_expiry": { "date": "2026-06-12T21:59:59", "amount": 16 }
            }
        })))
        .mount(&mock)
        .await;

    let snapshot: SharedSnapshot = Arc::new(RwLock::new(MetricsSnapshot::default()));
    {
        let client = client_for(&mock);
        let points = client.fetch_points().await.unwrap();
        let mut snap = snapshot.write().await;
        snap.last_attempt = Some(chrono::Utc::now());
        snap.last_success = snap.last_attempt;
        snap.points = Some(points);
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = server::router(snapshot);
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let metrics = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(metrics.contains("westpunkte_fetch_success 1"));
    assert!(metrics.contains("westpunkte_balance 221"));
    assert!(metrics.contains("westpunkte_expiring{expires_on=\"2026-06-12\"} 16"));
    assert!(metrics.contains("# TYPE westpunkte_balance gauge"));

    let health = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(health.status(), 200);

    server_handle.abort();
}

#[tokio::test]
async fn metrics_endpoint_returns_failure_indicator_when_never_fetched() {
    let snapshot: SharedSnapshot = Arc::new(RwLock::new(MetricsSnapshot::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = server::router(snapshot);
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let metrics = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(metrics.contains("westpunkte_fetch_success 0"));
    assert!(!metrics.contains("westpunkte_balance"));

    server_handle.abort();
}
