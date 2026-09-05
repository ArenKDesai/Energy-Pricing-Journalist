use serde::{Deserialize, Serialize};

/// The MISO HUB nodes we track. These are the `*.HUB` locations returned by the
/// DataBroker consolidated LMP feed, and the same set `db/src/export_history.py`
/// exports history for — keep the two lists in step.
pub const HUBS: [&str; 8] = [
    "ARKANSAS.HUB",
    "ILLINOIS.HUB",
    "INDIANA.HUB",
    "LOUISIANA.HUB",
    "MICHIGAN.HUB",
    "MINN.HUB",
    "MS.HUB",
    "TEXAS.HUB",
];

/// MISO real-time intervals are 5 minutes long.
pub const INTERVAL_SECONDS: i64 = 300;

/// The archive exports settled prices on an hourly grid, because that is the
/// granularity MISO's market reports publish.
pub const HISTORY_INTERVAL_SECONDS: i64 = 3600;

/// How much live 5-minute history to retain in IndexedDB. Anything older is
/// better served by the archive, which is denser in coverage and already settled.
pub const WINDOW_SECONDS: i64 = 3 * 24 * 60 * 60;

/// Selectable chart ranges (label, seconds). The longest must not exceed what
/// `export_history.py --days` was run with, or the chart will simply show less.
pub const RANGES: [(&str, i64); 4] = [
    ("3d", 3 * 86_400),
    ("7d", 7 * 86_400),
    ("14d", 14 * 86_400),
    ("30d", 30 * 86_400),
];

/// A point in data space: (epoch seconds, LMP $/MWh).
pub type Point = (i64, f64);

/// One settled 5-minute interval: a timestamp plus the LMP for each HUB.
///
/// `ts` is epoch seconds floored to the 5-minute interval boundary, so it
/// doubles as the IndexedDB primary key — re-fetching the same interval simply
/// overwrites it as MISO's value settles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub ts: i64,
    /// (hub name, LMP $/MWh) for every HUB present in the feed.
    pub prices: Vec<(String, f64)>,
}

impl Snapshot {
    pub fn lmp(&self, hub: &str) -> Option<f64> {
        self.prices
            .iter()
            .find(|(h, _)| h == hub)
            .map(|(_, v)| *v)
    }
}

/// Extract one hub's series from a run of snapshots, ascending by timestamp.
pub fn hub_series(snaps: &[Snapshot], hub: &str) -> Vec<Point> {
    snaps
        .iter()
        .filter_map(|s| s.lmp(hub).map(|v| (s.ts, v)))
        .collect()
}

/// Floor an epoch-seconds value to the start of its 5-minute interval.
pub fn floor_interval(epoch_secs: i64) -> i64 {
    epoch_secs - epoch_secs.rem_euclid(INTERVAL_SECONDS)
}

/// Current wall-clock time in epoch seconds (from the browser clock).
pub fn now_secs() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}
