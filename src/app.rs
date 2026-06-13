use std::cell::RefCell;
use std::rc::Rc;

use leptos::*;
use web_sys::IdbDatabase;

use crate::chart;
use crate::miso::fetch_snapshot;
use crate::model;
use crate::storage::{load_all, open_db, prune_before, put_snapshot};
use crate::types::{now_secs, Snapshot, HUBS, WINDOW_SECONDS};

/// Poll cadence. MISO publishes every 5 minutes; we poll more often and dedup
/// by interval so a new timestamp shows up promptly without missing it.
const POLL_MS: u32 = 60_000;
/// Intervals to forecast ahead (24 × 5 min = 2 hours).
const FORECAST_STEPS: usize = 24;

/// Drop the ".HUB" suffix for display.
fn pretty(hub: &str) -> String {
    hub.trim_end_matches(".HUB").to_string()
}

/// Insert a snapshot in timestamp order (overwriting a matching interval), then
/// trim anything older than the 3-day window.
fn merge(list: &mut Vec<Snapshot>, snap: Snapshot) {
    match list.binary_search_by_key(&snap.ts, |s| s.ts) {
        Ok(i) => list[i] = snap,
        Err(i) => list.insert(i, snap),
    }
    let cutoff = now_secs() - WINDOW_SECONDS;
    list.retain(|s| s.ts >= cutoff);
}

fn local_time_label() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "{:02}:{:02}:{:02}",
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds()
    )
}

/// Fetch one snapshot, persist it, and merge it into the live signal.
async fn refresh(
    snapshots: RwSignal<Vec<Snapshot>>,
    status: RwSignal<String>,
    error: RwSignal<Option<String>>,
    db: Rc<RefCell<Option<IdbDatabase>>>,
) {
    match fetch_snapshot().await {
        Ok(snap) => {
            if let Some(d) = db.borrow().clone() {
                let _ = put_snapshot(&d, &snap).await;
                let _ = prune_before(&d, now_secs() - WINDOW_SECONDS).await;
            }
            snapshots.update(|l| merge(l, snap));
            error.set(None);
            let n = snapshots.with_untracked(|l| l.len());
            status.set(format!("{n} samples · updated {}", local_time_label()));
        }
        Err(e) => error.set(Some(e)),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let snapshots = create_rw_signal::<Vec<Snapshot>>(Vec::new());
    let hub = create_rw_signal(HUBS[0].to_string());
    let loading = create_rw_signal(true);
    let status = create_rw_signal("Connecting…".to_string());
    let error = create_rw_signal::<Option<String>>(None);
    let resize_tick = create_rw_signal(0u32);

    let canvas_ref = create_node_ref::<html::Canvas>();
    let db: Rc<RefCell<Option<IdbDatabase>>> = Rc::new(RefCell::new(None));

    // Startup: open IndexedDB, hydrate from stored history, fetch, then poll.
    {
        let db = db.clone();
        spawn_local(async move {
            if let Ok(d) = open_db().await {
                *db.borrow_mut() = Some(d);
            }
            if let Some(d) = db.borrow().clone() {
                if let Ok(existing) = load_all(&d).await {
                    let cutoff = now_secs() - WINDOW_SECONDS;
                    snapshots.set(existing.into_iter().filter(|s| s.ts >= cutoff).collect());
                }
            }

            refresh(snapshots, status, error, db.clone()).await;
            if snapshots.with_untracked(|l| !l.is_empty()) {
                loading.set(false);
            }

            loop {
                gloo_timers::future::TimeoutFuture::new(POLL_MS).await;
                refresh(snapshots, status, error, db.clone()).await;
                if snapshots.with_untracked(|l| !l.is_empty()) {
                    loading.set(false);
                }
            }
        });
    }

    // Fit + forecast only when the data or selected hub changes (not on resize),
    // since fitting ARIMA-GARCH is the expensive step.
    let forecast = create_memo(move |_| {
        let snaps = snapshots.get();
        let h = hub.get();
        model::forecast(&snaps, &h, FORECAST_STEPS)
    });

    // Redraw whenever the data, selection, forecast, or viewport changes.
    create_effect(move |_| {
        resize_tick.track();
        let snaps = snapshots.get();
        let h = hub.get();
        let fc = forecast.get();
        if let Some(canvas) = canvas_ref.get() {
            chart::render(&canvas, &snaps, &h, &fc);
        }
    });

    window_event_listener(ev::resize, move |_| {
        resize_tick.update(|n| *n += 1);
    });

    view! {
        <div id="header">
            <h1>"MISO Real-Time LMP"</h1>
            <div id="controls">
                <label>
                    "HUB: "
                    <select on:change=move |ev| hub.set(event_target_value(&ev))>
                        {HUBS
                            .iter()
                            .map(|h| view! { <option value=*h>{pretty(h)}</option> })
                            .collect_view()}
                    </select>
                </label>
                <span class="status">{move || status.get()}</span>
                <span class="forecast-tag">{move || forecast.with(|f| f.label.clone())}</span>
            </div>
        </div>

        <div id="chart-container">
            <canvas node_ref=canvas_ref></canvas>
            <Show when=move || loading.get()>
                <div id="loading">
                    <div class="spinner"></div>
                    <div class="loading-text">
                        {move || match error.get() {
                            Some(e) => {
                                view! {
                                    <span class="error">"Couldn't reach MISO: " {e}</span>
                                    <br/>
                                    "Retrying every minute…"
                                }
                                    .into_view()
                            }
                            None => {
                                view! {
                                    "Downloading real-time HUB prices from MISO…"
                                    <br/>
                                    "History builds up to 3 days as new intervals arrive."
                                }
                                    .into_view()
                            }
                        }}
                    </div>
                </div>
            </Show>
        </div>
    }
}
