use std::fmt::Write;

use chrono::{DateTime, Utc};

use crate::client::PointsData;

#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub points: Option<PointsData>,
}

pub fn render(snapshot: &MetricsSnapshot, now: DateTime<Utc>) -> String {
    let mut out = String::with_capacity(1024);

    let success = u8::from(snapshot.last_error.is_none() && snapshot.last_success.is_some());
    metric(
        &mut out,
        "westpunkte_fetch_success",
        "1 if the last fetch from the Westbahn API succeeded, 0 otherwise.",
        success,
        None,
    );

    metric(
        &mut out,
        "westpunkte_last_attempt_timestamp_seconds",
        "Unix timestamp of the most recent fetch attempt.",
        snapshot.last_attempt.map(|t| t.timestamp()).unwrap_or(0),
        None,
    );

    metric(
        &mut out,
        "westpunkte_last_success_timestamp_seconds",
        "Unix timestamp of the most recent successful fetch.",
        snapshot.last_success.map(|t| t.timestamp()).unwrap_or(0),
        None,
    );

    if let Some(points) = &snapshot.points {
        metric(
            &mut out,
            "westpunkte_balance",
            "Current WestPunkte balance reported by the Westbahn customer API.",
            points.total,
            None,
        );

        if let Some(expiry) = &points.expiry {
            let label = format!("expires_on=\"{}\"", expiry.date);
            metric(
                &mut out,
                "westpunkte_expiring",
                "Number of WestPunkte expiring on the next expiry date.",
                expiry.amount,
                Some(&label),
            );

            let days_remaining = (expiry.date - now.date_naive()).num_days();
            metric(
                &mut out,
                "westpunkte_expiry_days_remaining",
                "Days until the next batch of WestPunkte expires (negative if overdue).",
                days_remaining,
                None,
            );
        }
    }

    out
}

fn metric<V: std::fmt::Display>(
    out: &mut String,
    name: &str,
    help: &str,
    value: V,
    labels: Option<&str>,
) {
    writeln!(out, "# HELP {name} {help}").unwrap();
    writeln!(out, "# TYPE {name} gauge").unwrap();
    match labels {
        Some(l) => writeln!(out, "{name}{{{l}}} {value}").unwrap(),
        None => writeln!(out, "{name} {value}").unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{ExpiryInfo, PointsData};
    use chrono::{NaiveDate, TimeZone};

    fn dt(s: &str) -> DateTime<Utc> {
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
        Utc.from_utc_datetime(&naive)
    }

    fn full_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            last_attempt: Some(dt("2026-06-06 12:00:00")),
            last_success: Some(dt("2026-06-06 12:00:00")),
            last_error: None,
            points: Some(PointsData {
                total: 182,
                expiry: Some(ExpiryInfo {
                    date: NaiveDate::from_ymd_opt(2026, 6, 12).unwrap(),
                    amount: 16,
                }),
            }),
        }
    }

    #[test]
    fn renders_all_metrics_when_data_is_present() {
        let snap = full_snapshot();
        let body = render(&snap, dt("2026-06-06 12:00:00"));

        assert!(body.contains("westpunkte_fetch_success 1"));
        assert!(body.contains("westpunkte_balance 182"));
        assert!(body.contains("westpunkte_expiring{expires_on=\"2026-06-12\"} 16"));
        assert!(body.contains("westpunkte_expiry_days_remaining 6"));
        // HELP/TYPE present for each metric
        for name in [
            "westpunkte_fetch_success",
            "westpunkte_last_attempt_timestamp_seconds",
            "westpunkte_last_success_timestamp_seconds",
            "westpunkte_balance",
            "westpunkte_expiring",
            "westpunkte_expiry_days_remaining",
        ] {
            assert!(
                body.contains(&format!("# HELP {name}")),
                "missing HELP for {name}\n{body}"
            );
            assert!(
                body.contains(&format!("# TYPE {name} gauge")),
                "missing TYPE for {name}"
            );
        }
    }

    #[test]
    fn returns_failure_indicator_when_no_success_yet() {
        let snap = MetricsSnapshot {
            last_attempt: Some(dt("2026-06-06 12:00:00")),
            last_error: Some("boom".into()),
            ..Default::default()
        };
        let body = render(&snap, dt("2026-06-06 12:00:00"));
        assert!(body.contains("westpunkte_fetch_success 0"));
        // No balance metric should be emitted when there is no data.
        assert!(!body.contains("westpunkte_balance"));
    }

    #[test]
    fn omits_expiry_metrics_when_none() {
        let mut snap = full_snapshot();
        snap.points.as_mut().unwrap().expiry = None;
        let body = render(&snap, dt("2026-06-06 12:00:00"));
        assert!(body.contains("westpunkte_balance 182"));
        assert!(!body.contains("westpunkte_expiring"));
        assert!(!body.contains("westpunkte_expiry_days_remaining"));
    }

    #[test]
    fn days_remaining_can_be_negative() {
        let snap = full_snapshot();
        let body = render(&snap, dt("2026-06-20 12:00:00"));
        assert!(
            body.contains("westpunkte_expiry_days_remaining -8"),
            "got: {body}"
        );
    }
}
