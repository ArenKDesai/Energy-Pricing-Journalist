//! The settled hourly archive, loaded as a static asset.
//!
//! MISO's market-report host (`docs.misoenergy.org`) sends no CORS headers, so
//! the browser cannot read real history itself — which is why this app used to
//! start empty and accumulate forward. `db/` downloads those reports into DuckDB
//! and `db/src/export_history.py` writes the hub slice of it to
//! `assets/hub_history.json`, which ships with the built site and is therefore
//! same-origin. Loading it gives a cold browser a full window of real prices
//! immediately, and gives the forecast something to fit on from the first frame.
//!
//! The archive is hourly and settled; the DataBroker feed is 5-minute and live.
//! They are deliberately kept as separate series rather than concatenated — see
//! `model.rs` for why mixing the two sampling rates would corrupt the fit.

use std::collections::HashMap;

use gloo_net::http::Request;
use serde::Deserialize;

use crate::types::Point;

/// Path is relative to the page, so this resolves against whatever origin the
/// static site is served from.
const ASSET: &str = "assets/hub_history.json";

/// Wire format written by `db/src/export_history.py`: column-oriented on an
/// implicit time grid, so only prices are stored and timestamps are derived.
#[derive(Deserialize)]
struct Wire {
    interval_seconds: i64,
    t0: i64,
    n: usize,
    rt_end: i64,
    da: HashMap<String, Vec<Option<f64>>>,
    rt: HashMap<String, Vec<Option<f64>>>,
}

/// Hourly settled history per hub, expanded onto explicit timestamps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub interval: i64,
    /// Timestamp of the newest settled RT hour — where live data takes over.
    pub rt_end: i64,
    pub da: HashMap<String, Vec<Point>>,
    pub rt: HashMap<String, Vec<Point>>,
}

impl History {
    /// Settled real-time series for one hub, ascending by timestamp.
    pub fn rt_series(&self, hub: &str) -> &[Point] {
        self.rt.get(hub).map_or(&[], |v| v.as_slice())
    }

    /// Settled day-ahead series for one hub, ascending by timestamp.
    pub fn da_series(&self, hub: &str) -> &[Point] {
        self.da.get(hub).map_or(&[], |v| v.as_slice())
    }

    /// Total hours spanned, for status display.
    pub fn hours(&self) -> usize {
        self.rt.values().map(|v| v.len()).max().unwrap_or(0)
    }
}

/// Expand one column into `(timestamp, price)` pairs, dropping missing hours.
fn expand(col: &[Option<f64>], t0: i64, interval: i64, n: usize) -> Vec<Point> {
    col.iter()
        .take(n)
        .enumerate()
        .filter_map(|(i, v)| v.map(|p| (t0 + i as i64 * interval, p)))
        .collect()
}

/// Fetch and parse the archive asset.
///
/// A missing or malformed asset is not fatal: the caller falls back to the
/// original accumulate-forward behaviour, so the app still works when deployed
/// without an export.
pub async fn load() -> Result<History, String> {
    let resp = Request::get(ASSET)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("history asset returned HTTP {}", resp.status()));
    }

    let wire: Wire = resp
        .json()
        .await
        .map_err(|e| format!("parse error: {e}"))?;

    let interval = if wire.interval_seconds > 0 {
        wire.interval_seconds
    } else {
        return Err("history asset has a non-positive interval".into());
    };

    let convert = |m: &HashMap<String, Vec<Option<f64>>>| -> HashMap<String, Vec<Point>> {
        m.iter()
            .map(|(hub, col)| (hub.clone(), expand(col, wire.t0, interval, wire.n)))
            .collect()
    };

    Ok(History {
        interval,
        rt_end: wire.rt_end,
        da: convert(&wire.da),
        rt: convert(&wire.rt),
    })
}
