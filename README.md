# Energy Pricing Journalist

MISO locational marginal prices, from a three-year settled archive through to
the live 5-minute feed, in one browser-only page.

Two halves that fit together:

| | |
|---|---|
| [`db/`](db) | A Python + DuckDB archive of MISO **Day-Ahead (ex-post)** and **Real-Time** nodal LMPs, 2023-01-01 to present, ~2,400 nodes, hourly. |
| [`web/`](web) | A **Rust + WebAssembly** single-page app that charts the live real-time HUB LMPs and forecasts the next few hours with ARIMA-GARCH. |

The bridge between them is [`db/src/export_history.py`](db/src/export_history.py),
which writes the hub slice of the archive to `web/assets/hub_history.json` — a
small static asset that ships with the site.

## Why the two halves need each other

The web app is browser-only by design: no backend, no server-side collector.
That constraint cost it history. MISO's `MISORTWDDataBroker` endpoint is the
only feed that sends `Access-Control-Allow-Origin: *`, and it returns just the
*current* 5-minute snapshot. The market-report files that contain real history
live on a host with **no CORS headers at all**, so the page could never read
them. The app therefore used to start from a single point on every fresh browser
profile and accumulate forward, taking three days to fill its own window and
about four hours before it had enough samples to fit its forecast model at all.

`db/` already downloads exactly those market reports. Exporting the eight
trading hubs from it as a same-origin JSON asset closes the gap: a cold browser
now paints a full window of real settled prices on first load, and the
ARIMA-GARCH fit is available from the first frame.

Nothing about the deployment model changes — `dist/` is still a fully static
site with nothing to run server-side.

## Quick start

```bash
# 1. Build the archive (see db/ below; skip if data/miso_lmp.duckdb exists)
db/.venv/Scripts/python.exe db/src/fetch.py
db/.venv/Scripts/python.exe db/src/load.py

# 2. Export the hub slice the web app loads
db/.venv/Scripts/python.exe db/src/export_history.py --days 30

# 3. Run the app
cd web && trunk serve      # http://127.0.0.1:8080
```

---

## `db/` — the archive

### Coverage

| | |
|---|---|
| Start | **2023-01-01** |
| End | present (rolling) |
| Granularity | hourly (hour-ending 1-24) |
| Nodes | ~2,400 (Hub, Loadzone, Interface, Gennode) |
| Components | `lmp`, `mcc` (congestion), `mlc` (loss) |

### Why history starts in 2023

This is a limit of the publisher, not of this tool. MISO serves nodal LMP
reports from `docs.misoenergy.org/marketreports` in two forms — daily CSVs and
quarterly "Historical Annual DA/RT LMPs" ZIPs. Both were probed across every
year from 2005 to present, under both observed filename conventions (`_LMP.zip`
and `_LMPs.zip`); everything before `2023-01-01` returns 404. MISO has also
announced these reports stop being produced after 2025-12-12, with data moving
to the MISO Data Exchange API. Pre-2023 nodal history is not available from this
public endpoint and would need a commercial source (Yes Energy, Velocity Suite,
gridstatus.io) or a MISO Data Exchange subscription.

### Setup

```bash
cd db
uv venv --python 3.12
uv pip install httpx duckdb
```

### Build

```bash
# 1. download raw reports (resumable; safe to re-run)
db/.venv/Scripts/python.exe db/src/fetch.py

# 2. reshape into DuckDB
db/.venv/Scripts/python.exe db/src/load.py

# 3. sanity-check coverage
db/.venv/Scripts/python.exe db/src/verify.py
```

To update later, re-run all of them — `fetch.py` skips days already on disk and
`load.py` replaces data month-by-month, so incremental refresh is just a re-run.
Then re-run `export_history.py` and rebuild the site to publish the new history.

Neither `db/data/` nor `db/.venv/` is committed: the raw reports are ~600 MB and
the DuckDB is ~2.3 GB, and both rebuild from the two scripts above.

### Schema

`lmp` — one row per market / node / hour:

| column | notes |
|---|---|
| `market` | `DA` or `RT` |
| `ts_est` | hour-**beginning** timestamp |
| `he` | hour-ending 1-24, as published |
| `node`, `node_type` | node_type is Hub / Loadzone / Interface / Gennode |
| `lmp`, `mcc`, `mlc` | $/MWh |

**Timezone:** MISO publishes these reports in EST year-round and does not shift
for daylight saving, so every day has exactly 24 hours and there are no missing
or duplicated spring/fall hours. `ts_est` is stored as a naive timestamp meaning
EST (UTC-5). Convert with `ts_est AT TIME ZONE 'EST'` if you need UTC.

Views: `nodes` (node directory + coverage), `da_rt_spread` (DA and RT side by
side with `da_minus_rt`), `hub_prices`, and `major_hubs`.

> `hub_prices` is every node MISO tags with `node_type = 'Hub'` — that is ~445
> commercial pricing nodes, not the trading hubs. **`major_hubs`** is the eight
> `*.HUB` trading hubs the web app actually charts. Filtering on `node_type`
> when you mean the trading hubs is the easy mistake here.

### Source files

- DA: `{YYYYMMDD}_da_expost_lmp.csv`
- RT: `{YYYYMMDD}_rt_lmp_final.csv`, falling back to `_rt_lmp_prelim.csv` for
  recent days where settlement has not finalized. The variant actually used for
  each day is recorded in `db/data/fetch_manifest.json`.

---

## `web/` — the app

On load it reads the settled archive asset, draws it, connects to MISO's
DataBroker, and keeps fetching every new 5-minute interval on top.

### Data flow

- **Settled history.** `assets/hub_history.json`, written by
  `export_history.py`. Column-oriented on an implicit hourly grid — only prices
  are stored and timestamps are derived — so 30 days of DA+RT for 8 hubs is
  about 69 KB. Same-origin, so no CORS problem.
- **Live prices.** MISO's `MISORTWDDataBroker` `GetDataByNodeTypes` endpoint,
  the one MISO feed that sends `Access-Control-Allow-Origin: *`. It returns the
  *current* 5-minute snapshot for all nodes; we keep the 8 `*.HUB` locations
  (ARKANSAS, ILLINOIS, INDIANA, LOUISIANA, MICHIGAN, MINN, MS, TEXAS).
- **Live persistence.** 5-minute snapshots go to **IndexedDB**, keyed by
  interval timestamp so late-settling values overwrite in place, pruned to 3
  days. Anything older is better served by the archive.
- **Polling.** Once a minute, de-duplicated by interval timestamp.

### What the chart draws

Four things share one canvas for the selected hub, and a legend names whichever
of them are actually present:

| | |
|---|---|
| **Real-time, live** | full-weight line, 5-minute samples from the DataBroker |
| **Real-time, settled** | thin muted line, hourly, from the archive |
| **Day-ahead, settled** | dashed blue line, hourly, from the archive |
| **Forecast** | dashed amber mean path inside a translucent 95% band |

Day-ahead is on by default — the DA/RT spread is the comparison the page exists
to make, so it should be on screen without a click — and the checkbox in the
header turns it off. Real-time is stroked with a price-dependent gradient rather
than a flat colour, so its legend swatch is neutral and the live and settled
series are told apart by weight, exactly as they read on the chart.

### The settlement gap

RT reports settle a few days late, so the archive ends where settlement ends —
typically a day or two before now — while the live feed starts at the present
moment. The chart draws the two as **separate paths** and skips any segment
wider than its expected sampling interval, so that gap reads as a gap rather
than an invented straight line across it. The header shows how far the archive
actually runs.

### Forecast

[`web/src/model.rs`](web/src/model.rs) fits an **ARIMA(p,1,q) + GARCH(1,1)**
model entirely client-side:

- *Mean.* First-difference the price series (d = 1), then ARMA(p, q) on the
  differences with orders chosen by AIC and parameters fit by conditional least
  squares. The forecast is rolled forward and re-integrated to price levels.
- *Variance.* GARCH(1,1) on the ARMA residuals (Gaussian MLE), propagated
  through the ARIMA MA(∞) weights so the 95% band widens with horizon and with
  recent volatility.
- *Fitting* uses a small Nelder–Mead optimiser; both fits are plain `f64` and
  run in WASM.

**Two sampling grids.** The archive is hourly and the live feed is 5-minute.
They are never concatenated or interpolated onto a common grid, because that
would break the model twice over: the ARMA recursion indexes by position and so
assumes even spacing, and GARCH would be estimating 5-minute conditional
volatility from hourly moves — understating it badly, since intra-hour variance
is exactly what hourly settlement averages away. Instead the model fits
whichever series is adequate on its own grid:

| samples available | fits on | horizon |
|---|---|---|
| ≥ 48 live 5-minute samples | live feed | 2 h (24 × 5 min) |
| fewer, with the archive loaded | hourly archive | 6 h (6 × 1 h) |
| neither | random walk with drift | — |

The chart tag always names the basis actually used, so it is visible at a glance
which grid produced the band. The random-walk fallback now only appears if the
archive asset fails to load.

### Project layout

| File | Responsibility |
|---|---|
| [web/src/main.rs](web/src/main.rs) | Entry point; mounts the Leptos app |
| [web/src/app.rs](web/src/app.rs) | Root component, startup, polling loop, controls |
| [web/src/history.rs](web/src/history.rs) | Load + expand the settled archive asset |
| [web/src/miso.rs](web/src/miso.rs) | Fetch + parse the live DataBroker snapshot |
| [web/src/storage.rs](web/src/storage.rs) | IndexedDB persistence + 3-day pruning |
| [web/src/chart.rs](web/src/chart.rs) | Canvas2D rendering |
| [web/src/model.rs](web/src/model.rs) | ARIMA-GARCH forecast (fit + predict) |
| [web/src/types.rs](web/src/types.rs) | Shared types, hub list, time helpers |

### Development

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
cd web
trunk serve   # http://127.0.0.1:8080
```

**Build for deployment** (static files into `web/dist/`)

```bash
cd web
trunk build --release
```

`web/assets/` is copied into `dist/` by Trunk, so the exported history ships
with the bundle. The contents of `dist/` are a fully static site — host them
anywhere (GCS, Netlify, GitHub Pages). There is nothing to run server-side.

### Deployment

[`.github/workflows/deploy.yml`](.github/workflows/deploy.yml) builds the site
and publishes it to **GitHub Pages** on every push to `main`, and can be run by
hand from the Actions tab. Pull requests run the same build without deploying,
so a broken build is caught before it lands.

Pages serves a project site under `/<repo>/` rather than the domain root, so the
workflow builds with `--public-url "/$REPO/"`, which rewrites the `.js` and
`.wasm` URLs Trunk emits into `index.html` to absolute `/<repo>/…` paths.
`history.rs` requests `assets/hub_history.json` relatively and so is resolved by
the browser against the page URL, which is correct at either depth and needs no
build-time flag.

The workflow does **not** run `db/`. Rebuilding the archive means downloading
~600 MB of market reports into a ~2.3 GB DuckDB, and neither is committed.
Refreshing the published history is a local step — re-run `fetch.py`, `load.py`
and `export_history.py`, then commit the regenerated
`web/assets/hub_history.json`. Pushing that commit is what redeploys the site
with newer settled data.
