use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;

const USER_AGENT: &str = concat!(
    "westpunkte-metrics/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/yolgie/westpunkte-metrics)"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointsData {
    pub total: i64,
    pub expiry: Option<ExpiryInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiryInfo {
    pub date: NaiveDate,
    pub amount: i64,
}

pub struct WestbahnClient {
    http: reqwest::Client,
    base_url: String,
    auth_token: String,
    auth_id_email: String,
}

impl WestbahnClient {
    pub fn new(
        base_url: impl Into<String>,
        auth_token: impl Into<String>,
        auth_id_email: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth_token: auth_token.into(),
            auth_id_email: auth_id_email.into(),
        })
    }

    pub async fn fetch_points(&self) -> Result<PointsData> {
        let url = format!("{}/api/v1/customer", self.base_url);
        let cookie = format!(
            "customer_auth_token={}; customer_auth_id_email={}",
            self.auth_token, self.auth_id_email
        );
        let resp = self
            .http
            .get(&url)
            .header("Cookie", cookie)
            .header("Accept", "application/json")
            .header("Accept-Language", "de-AT")
            .send()
            .await
            .context("Failed to call Westbahn API")?;

        let status = resp.status();
        let body = resp.text().await.context("Failed to read response body")?;

        if !status.is_success() {
            bail!(
                "Westbahn API returned HTTP {}: {}",
                status,
                snippet(&body, 200)
            );
        }

        parse_response(&body)
    }
}

fn parse_response(body: &str) -> Result<PointsData> {
    let parsed: CustomerEnvelope = serde_json::from_str(body)
        .with_context(|| format!("Failed to decode response body: {}", snippet(body, 200)))?;
    if !parsed.success {
        bail!(
            "Westbahn API reported failure: {}",
            parsed.message.as_deref().unwrap_or("(no message)")
        );
    }
    let customer = parsed
        .customer
        .context("Westbahn API succeeded but returned no customer payload")?;

    let expiry = match customer.westpunkte_expiry {
        Some(raw) if raw.amount > 0 => Some(parse_expiry(raw)?),
        _ => None,
    };

    Ok(PointsData {
        total: customer.westpunkte,
        expiry,
    })
}

fn parse_expiry(raw: RawExpiry) -> Result<ExpiryInfo> {
    let date_part = raw.date.split('T').next().unwrap_or(&raw.date);
    let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
        .with_context(|| format!("Failed to parse expiry date {:?}", raw.date))?;
    Ok(ExpiryInfo {
        date,
        amount: raw.amount,
    })
}

fn snippet(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[derive(Deserialize)]
struct CustomerEnvelope {
    success: bool,
    customer: Option<CustomerPayload>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct CustomerPayload {
    westpunkte: i64,
    westpunkte_expiry: Option<RawExpiry>,
}

#[derive(Deserialize)]
struct RawExpiry {
    date: String,
    amount: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance_with_expiry() {
        let body = r#"{
            "success": true,
            "customer": {
                "westpunkte": 182,
                "westpunkte_expiry": { "date": "2026-06-12T21:59:59", "amount": 16 }
            },
            "message": "ok"
        }"#;
        let data = parse_response(body).unwrap();
        assert_eq!(data.total, 182);
        assert_eq!(
            data.expiry,
            Some(ExpiryInfo {
                date: NaiveDate::from_ymd_opt(2026, 6, 12).unwrap(),
                amount: 16
            })
        );
    }

    #[test]
    fn omits_expiry_when_amount_is_zero() {
        let body = r#"{
            "success": true,
            "customer": {
                "westpunkte": 50,
                "westpunkte_expiry": { "date": "2027-01-01T00:00:00", "amount": 0 }
            }
        }"#;
        let data = parse_response(body).unwrap();
        assert_eq!(data.total, 50);
        assert!(data.expiry.is_none());
    }

    #[test]
    fn omits_expiry_when_missing() {
        let body = r#"{
            "success": true,
            "customer": { "westpunkte": 0, "westpunkte_expiry": null }
        }"#;
        let data = parse_response(body).unwrap();
        assert_eq!(data.total, 0);
        assert!(data.expiry.is_none());
    }

    #[test]
    fn surfaces_api_failure_message() {
        let body = r#"{ "success": false, "message": "session expired" }"#;
        let err = parse_response(body).unwrap_err().to_string();
        assert!(err.contains("session expired"), "got: {err}");
    }

    #[test]
    fn rejects_malformed_response() {
        let err = parse_response("<html>nope</html>").unwrap_err().to_string();
        assert!(err.contains("Failed to decode response body"), "got: {err}");
    }
}
