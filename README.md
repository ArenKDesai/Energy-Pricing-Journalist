# Energy Pricing Journalist

A **Rust + WebAssembly** single-page app that streams MISO real-time HUB LMPs
**entirely in the browser** — no backend, no server-side collector.

On load it connects to MISO's DataBroker, draws a live line chart of the
real-time LMP for each HUB, and keeps fetching every new 5-minute interval.
History is persisted locally (IndexedDB) and accumulates toward a rolling
3-day window across reloads.

## How it works

- **Data source.** MISO's `MISORTWDDataBroker` `GetDataByNodeTypes` endpoint is
  the only MISO feed that sends `Access-Control-Allow-Origin: *`, so the browser
  can call it directly with no proxy. It returns the *current* 5-minute snapshot
  for all nodes; we keep the 8 `*.HUB` locations (ARKANSAS, ILLINOIS, INDIANA,
  LOUISIANA, MICHIGAN, MINN, MS, TEXAS).
- **History.** The market-report files that contain true history live on a
  host with **no CORS**, and are only hourly — unreachable from a browser-only
  app. So instead of backfilling, the app accumulates 5-minute snapshots
  forward and stores them in **IndexedDB**, pruned to the last 3 days. A fresh
  browser profile therefore starts with one point and fills in over time; a
  returning visitor sees the history it has already collected.
- **Polling.** Polls once a minute and de-duplicates by interval timestamp, so a
  new 5-minute value appears promptly and late-settling values overwrite in
  place.
- **Chart.** Canvas2D line chart with axes, high/low markers, the latest value,
  and the forecast overlay (dashed mean path + translucent 95% band).
- **Forecast.** [`src/model.rs`](src/model.rs) fits an **ARIMA(p,1,q) + GARCH(1,1)**
  model entirely client-side and predicts the next 2 hours (24 × 5-minute steps):
  - *Mean.* First-difference the price series (d = 1), then ARMA(p, q) on the
    differences with orders chosen by AIC and parameters fit by conditional least
    squares. The forecast is rolled forward and re-integrated to price levels.
  - *Variance.* GARCH(1,1) on the ARMA residuals (Gaussian MLE), propagated
    through the ARIMA MA(∞) weights so the 95% band widens with horizon and with
    recent volatility.
  - *Fitting* uses a small Nelder–Mead optimiser; both fits are plain `f64` and
    run in WASM. Until ~48 samples have accumulated it falls back to a random
    walk with drift so the overlay still appears while history builds.

## Project layout

| File | Responsibility |
|---|---|
| [src/main.rs](src/main.rs) | Entry point; mounts the Leptos app |
| [src/app.rs](src/app.rs) | Root component, loading state, polling loop, wiring |
| [src/miso.rs](src/miso.rs) | Fetch + parse the DataBroker snapshot |
| [src/storage.rs](src/storage.rs) | IndexedDB persistence + 3-day pruning |
| [src/chart.rs](src/chart.rs) | Canvas2D rendering |
| [src/model.rs](src/model.rs) | ARIMA-GARCH forecast (fit + predict) |
| [src/types.rs](src/types.rs) | Shared types and time helpers |

## Development

**Prerequisites**

```bash
rustup target add wasm32-unknown-unknown
# Arch: a prebuilt binary avoids a known libdeflate/gcc-16 build failure
sudo pacman -S trunk
# elsewhere:
cargo install trunk
```

**Run**

```bash
trunk serve   # http://127.0.0.1:8080
```

A spinner ("Downloading real-time HUB prices from MISO…") shows until the first
snapshot lands, so it's always clear the page has loaded and is fetching.

**Build for deployment** (static files into `dist/`)

```bash
trunk build --release
```

The contents of `dist/` are a fully static site — host them anywhere (GCS,
Netlify, GitHub Pages, etc.). There is nothing to run server-side.
