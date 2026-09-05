use std::cell::RefCell;
use std::rc::Rc;

use leptos::*;
use web_sys::IdbDatabase;

use crate::chart::{self, ChartData};
use crate::history::{self, History};
use crate::miso::fetch_snapshot;
use crate::model;
use crate::storage::{load_all, open_db, prune_before, put_snapshot};
use crate::types::{hub_series, now_secs, Snapshot, HUBS, RANGES, WINDOW_SECONDS};

/// Poll cadence. MISO publishes every 5 minutes; we poll more often and dedup
/// by interval so a new timestamp shows up promptly without missing it.
const POLL_MS: u32 = 60_000;

/// Drop the ".HUB" suffix for display.
fn pretty(hub: &str) -> String {
    hub.trim_end_matches(".HUB").to_string()
}

/// Insert a snapshot in timestamp order (overwriting a matching interval), then
/// trim anything older than the live retention window.
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

/// "9/3 23:00" for an epoch-seconds instant, in the viewer's local zone.
fn stamp(ts: i64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((ts * 1000) as f64));
    format!(
        "{}/{} {:02}:{:02}",
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes()
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
            status.set(format!("{n} live samples · updated {}", local_time_label()));
        }
        Err(e) => error.set(Some(e)),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let snapshots = create_rw_signal::<Vec<Snapshot>>(Vec::new());
    let history = create_rw_signal(History::default());
    let hub = create_rw_signal(HUBS[0].to_string());
    let range = create_rw_signal(RANGES[0].1);
    // Day-ahead is shown alongside real-time by default: the DA/RT spread is
    // the comparison this page exists to make, so it should be on screen
    // without a click.
    let show_da = create_rw_signal(true);
    let loading = create_rw_signal(true);
    let status = create_rw_signal("Connecting…".to_string());
    let archive_status = create_rw_signal("loading archive…".to_string());
    let error = create_rw_signal::<Option<String>>(None);
    let resize_tick = create_rw_signal(0u32);

    let canvas_ref = create_node_ref::<html::Canvas>();
    let db: Rc<RefCell<Option<IdbDatabase>>> = Rc::new(RefCell::new(None));

    // Startup: load the settled archive, hydrate live history from IndexedDB,
    // then fetch and poll.
    {
        let db = db.clone();
        spawn_local(async move {
            // The archive is a small same-origin asset, so it lands well before
            // the first DataBroker response and paints a full chart immediately.
            // Failure is not fatal — without it the app simply behaves as it did
            // before, accumulating 5-minute samples forward.
            match history::load().await {
                Ok(h) => {
                    let hours = h.hours();
                    let through = stamp(h.rt_end);
                    archive_status.set(format!("archive: {hours} h settled through {through}"));
                    history.set(h);
                    loading.set(false);
                }
                Err(e) => archive_status.set(format!("archive unavailable ({e})")),
            }

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

    // Fit + forecast only when the data or selected hub changes (not on resize
    // or range change), since fitting ARIMA-GARCH is the expensive step.
    let forecast = create_memo(move |_| {
        let h = hub.get();
        let live = snapshots.with(|s| hub_series(s, &h));
        history.with(|hist| model::forecast_hub(&live, hist.rt_series(&h)))
    });

    // Redraw whenever the data, selection, range, forecast, or viewport changes.
    create_effect(move |_| {
        resize_tick.track();
        let h = hub.get();
        let range_secs = range.get();
        let want_da = show_da.get();
        let live = snapshots.with(|s| hub_series(s, &h));
        let fc = forecast.get();
        if let Some(canvas) = canvas_ref.get() {
            history.with(|hist| {
                let da = if want_da {
                    Some(hist.da_series(&h))
                } else {
                    None
                };
                let data = ChartData {
                    rt_archive: hist.rt_series(&h),
                    rt_live: &live,
                    da_archive: da,
                    range_secs,
                };
                chart::render(&canvas, &data, &fc);
            });
        }
    });

    window_event_listener(ev::resize, move |_| {
        resize_tick.update(|n| *n += 1);
    });

    view! {
        <div id="header">
            <h1>"MISO LMP — Real-Time vs Day-Ahead"</h1>
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
                <label>
                    "Range: "
                    <select on:change=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                            range.set(v);
                        }
                    }>
                        {RANGES
                            .iter()
                            .map(|(label, secs)| {
                                view! { <option value=secs.to_string()>{*label}</option> }
                            })
                            .collect_view()}
                    </select>
                </label>
                <label class="da-toggle">
                    <input
                        type="checkbox"
                        prop:checked=move || show_da.get()
                        on:change=move |ev| show_da.set(event_target_checked(&ev))
                    />
                    " Day-ahead"
                </label>
                <span class="status">{move || status.get()}</span>
                <span class="archive">{move || archive_status.get()}</span>
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
                                    "Loading settled history and connecting to MISO…"
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
