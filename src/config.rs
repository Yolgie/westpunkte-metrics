use std::env::{self, VarError};
use std::time::Duration;

use anyhow::{Context, Result};

const DEFAULT_PORT: u16 = 9090;
const DEFAULT_REFRESH_SECS: u64 = 300;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_BASE_URL: &str = "https://westbahn.at";

#[derive(Debug, Clone)]
pub struct Config {
    pub auth_token: String,
    pub auth_id_email: String,
    pub port: u16,
    pub refresh_interval: Duration,
    pub request_timeout: Duration,
    pub base_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_provider(|key| match env::var(key) {
            Ok(v) => Some(v),
            Err(VarError::NotPresent) => None,
            Err(VarError::NotUnicode(_)) => Some(String::new()),
        })
    }

    pub fn from_provider<F>(get: F) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let auth_token = required(&get, "WESTBAHN_AUTH_TOKEN")?;
        let auth_id_email = required(&get, "WESTBAHN_AUTH_ID_EMAIL")?;
        let port = parse_with_default(&get, "PORT", DEFAULT_PORT)?;
        let refresh_secs = parse_with_default(&get, "REFRESH_INTERVAL_SECS", DEFAULT_REFRESH_SECS)?;
        let timeout_secs = parse_with_default(&get, "REQUEST_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS)?;
        let base_url = get("WESTBAHN_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            auth_token,
            auth_id_email,
            port,
            refresh_interval: Duration::from_secs(refresh_secs),
            request_timeout: Duration::from_secs(timeout_secs),
            base_url,
        })
    }
}

fn required<F: Fn(&str) -> Option<String>>(get: &F, key: &str) -> Result<String> {
    let value = get(key).with_context(|| format!("{key} is not set"))?;
    if value.is_empty() {
        anyhow::bail!("{key} must not be empty");
    }
    Ok(value)
}

fn parse_with_default<F, T>(get: &F, key: &str, default: T) -> Result<T>
where
    F: Fn(&str) -> Option<String>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match get(key) {
        None => Ok(default),
        Some(raw) => raw
            .parse()
            .map_err(|e| anyhow::anyhow!("{key} is invalid: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn provider(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn applies_defaults_when_only_required_vars_present() {
        let cfg = Config::from_provider(provider(&[
            ("WESTBAHN_AUTH_TOKEN", "tok"),
            ("WESTBAHN_AUTH_ID_EMAIL", "1:e@x"),
        ]))
        .unwrap();
        assert_eq!(cfg.auth_token, "tok");
        assert_eq!(cfg.auth_id_email, "1:e@x");
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(
            cfg.refresh_interval,
            Duration::from_secs(DEFAULT_REFRESH_SECS)
        );
        assert_eq!(
            cfg.request_timeout,
            Duration::from_secs(DEFAULT_TIMEOUT_SECS)
        );
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn overrides_defaults_from_provider() {
        let cfg = Config::from_provider(provider(&[
            ("WESTBAHN_AUTH_TOKEN", "tok"),
            ("WESTBAHN_AUTH_ID_EMAIL", "1:e@x"),
            ("PORT", "8080"),
            ("REFRESH_INTERVAL_SECS", "60"),
            ("REQUEST_TIMEOUT_SECS", "10"),
            ("WESTBAHN_BASE_URL", "http://localhost"),
        ]))
        .unwrap();
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.refresh_interval, Duration::from_secs(60));
        assert_eq!(cfg.request_timeout, Duration::from_secs(10));
        assert_eq!(cfg.base_url, "http://localhost");
    }

    #[test]
    fn missing_required_var_is_rejected() {
        let err = Config::from_provider(provider(&[("WESTBAHN_AUTH_ID_EMAIL", "1:e@x")]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("WESTBAHN_AUTH_TOKEN"), "got: {err}");
    }

    #[test]
    fn empty_required_var_is_rejected() {
        let err = Config::from_provider(provider(&[
            ("WESTBAHN_AUTH_TOKEN", ""),
            ("WESTBAHN_AUTH_ID_EMAIL", "1:e@x"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(err.contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn invalid_numeric_var_is_rejected() {
        let err = Config::from_provider(provider(&[
            ("WESTBAHN_AUTH_TOKEN", "tok"),
            ("WESTBAHN_AUTH_ID_EMAIL", "1:e@x"),
            ("PORT", "not-a-port"),
        ]))
        .unwrap_err()
        .to_string();
        assert!(err.contains("PORT is invalid"), "got: {err}");
    }
}
