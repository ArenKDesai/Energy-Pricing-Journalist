use serde::{Deserialize, Serialize};

/// The MISO HUB nodes we track. These are the `*.HUB` locations returned by the
/// DataBroker consolidated LMP feed.
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

/// Rolling display window: 3 days of 5-minute samples.
pub const WINDOW_SECONDS: i64 = 3 * 24 * 60 * 60;

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

/// Floor an epoch-seconds value to the start of its 5-minute interval.
pub fn floor_interval(epoch_secs: i64) -> i64 {
    epoch_secs - epoch_secs.rem_euclid(INTERVAL_SECONDS)
}

/// Current wall-clock time in epoch seconds (from the browser clock).
pub fn now_secs() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}
