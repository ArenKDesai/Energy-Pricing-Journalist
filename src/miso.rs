//! Fetching live RT LMP snapshots directly from MISO's DataBroker.
//!
//! The `GetDataByNodeTypes` endpoint sends `Access-Control-Allow-Origin: *`, so
//! the browser can call it directly with no proxy. It returns the *current*
//! 5-minute interval only (parallel arrays keyed by metric), with no embedded
//! timestamp — we stamp it with the current floored interval ourselves.

use gloo_net::http::Request;
use serde::Deserialize;

use crate::types::{floor_interval, now_secs, Snapshot, HUBS};

const ENDPOINT: &str =
    "https://api.misoenergy.org/MISORTWDDataBroker/DataBrokerServices.asmx";
const PAYLOAD: &str =
    r#"{"messageType":"GetDataByNodeTypes","clientMessage":{"nodeTypes":["GEN","INT","LZN"]}}"#;

#[derive(Deserialize)]
struct BrokerResponse {
    data: BrokerData,
}

#[derive(Deserialize)]
struct BrokerData {
    #[serde(rename = "LMP")]
    lmp: Vec<f64>,
    #[serde(rename = "Location")]
    location: Vec<String>,
}

/// Fetch the current MISO snapshot and reduce it to the HUB LMPs.
pub async fn fetch_snapshot() -> Result<Snapshot, String> {
    // Use text/plain so this stays a CORS "simple request" and the browser
    // skips the preflight. MISO's OPTIONS response duplicates the
    // Access-Control-Allow-Origin header ("*, *"), which browsers reject — but
    // its actual POST response sends a single, valid header. The ASMX broker
    // reads the JSON body regardless of declared content type.
    let resp = Request::post(ENDPOINT)
        .header("Content-Type", "text/plain;charset=UTF-8")
        .body(PAYLOAD)
        .map_err(|e| format!("build request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("MISO returned HTTP {}", resp.status()));
    }

    let parsed: BrokerResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse error: {e}"))?;

    let data = parsed.data;
    if data.lmp.len() != data.location.len() {
        return Err("malformed MISO payload (array length mismatch)".into());
    }

    let mut prices: Vec<(String, f64)> = Vec::with_capacity(HUBS.len());
    for (loc, lmp) in data.location.iter().zip(data.lmp.iter()) {
        if HUBS.contains(&loc.as_str()) {
            prices.push((loc.clone(), *lmp));
        }
    }

    if prices.is_empty() {
        return Err("no HUB rows found in MISO feed".into());
    }

    Ok(Snapshot {
        ts: floor_interval(now_secs()),
        prices,
    })
}
