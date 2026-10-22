//! Anonymous usage counts for `GET /stats`.
//!
//! Each completed verification adds 1 to a single counter keyed by (UTC date, incident, verdict).
//! Nothing else is stored: no transaction id, no IP address, no time of day, no request details. A check
//! against a height that isn't a registry incident is counted as "custom height", without the height.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

pub const CUSTOM_HEIGHT: &str = "custom height";
pub const WHAT_IS_COUNTED: &str = "Only daily totals of completed checks, by verdict and by incident. \
     No transaction ids, IP addresses, times of day or other request details are recorded.";

pub struct Stats {
    conn: Mutex<Connection>,
}

/// Today's date in UTC, `YYYY-MM-DD` (civil-from-days, Howard Hinnant).
fn utc_day() -> String {
    let days = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

impl Stats {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("opening stats {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS daily_checks (
                 day TEXT NOT NULL, incident TEXT NOT NULL, verdict TEXT NOT NULL, count INTEGER NOT NULL,
                 PRIMARY KEY (day, incident, verdict));",
        )?;
        Ok(Stats { conn: Mutex::new(conn) })
    }

    /// Count one completed check. Errors are swallowed: counting must never fail a verification.
    pub fn count(&self, incident: &str, verdict: &str) {
        let r = self.conn.lock().unwrap().execute(
            "INSERT INTO daily_checks (day, incident, verdict, count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (day, incident, verdict) DO UPDATE SET count = count + 1",
            params![utc_day(), incident, verdict],
        );
        if let Err(e) = r {
            eprintln!("stats: {e}");
        }
    }

    pub fn summary(&self) -> Result<Value> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT day, incident, verdict, count FROM daily_checks ORDER BY day")?;
        let rows: Vec<(String, String, String, u64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        let mut total = 0;
        let mut by_verdict: BTreeMap<&str, u64> = BTreeMap::new();
        let mut by_incident: BTreeMap<&str, u64> = BTreeMap::new();
        let mut days: BTreeMap<&str, BTreeMap<&str, u64>> = BTreeMap::new();
        for (day, incident, verdict, n) in &rows {
            total += n;
            *by_verdict.entry(verdict).or_default() += n;
            *by_incident.entry(incident).or_default() += n;
            let d = days.entry(day).or_default();
            *d.entry("total").or_default() += n;
            *d.entry(verdict).or_default() += n;
        }
        Ok(json!({
            "what_is_counted": WHAT_IS_COUNTED,
            "since": rows.first().map(|r| r.0.clone()),
            "total_checks": total,
            "by_verdict": by_verdict,
            "by_incident": by_incident,
            "days": days.into_iter().map(|(day, c)| json!({ "day": day, "counts": c })).collect::<Vec<_>>(),
        }))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_day_format() {
        let d = super::utc_day();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }
}
