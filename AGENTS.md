# Project Guidelines

## Code Style
- Use Python 3.10+ compatible code unless a file explicitly requires newer syntax.
- Prefer `polars` for ETL/data-shaping paths in `src/`; use `pandas` only where required (Dash rendering and Chronos `predict_df` interop).
- Keep dataframe columns lowercase and preserve existing schema conventions (`location`, `datetime`, `lmp`, `mcc`, `mlc`, `status_code`).
- Keep timezone handling explicit and consistent with `America/Chicago` when transforming timestamps.
- Follow existing module boundaries rather than adding cross-cutting helpers in `main.py`.

## Architecture
- `main.py`: backend orchestrator for ingest -> store -> forecast -> parquet export.
- `src/pricing.py`: real-time MISO fetch logic and normalization to Polars records.
- `src/database.py`: DuckDB persistence (`pricing_history` table).
- `src/prediction.py`: interpolation + Chronos-2 inference and forecast dataframe assembly.
- `app.py`: Dash UI that reads `plot.parquet` from Cloudflare R2 and renders history + quantile bands.
- `src/historic.py` and `src/location.py`: historical backfill and node metadata utilities; keep separate from real-time fetch pipeline.

## Build and Test
- Install dependencies: `pip install -r requirements.txt`
- Run backend pipeline: `python main.py`
- Run dashboard locally: `python app.py`
- Container entrypoint is `main.py` via `uv` (`Dockerfile`).
- The README references `docker compose up -d`, but no compose file is currently in the repo; verify local workflow before relying on compose commands.
- There is no automated pytest suite yet; validate changes with focused script runs and/or notebook checks in `testing.ipynb`.

## Conventions
- Treat external services as first-class dependencies: MISO API, Cloudflare R2, and Chronos model downloads can fail or be slow.
- Keep forecasting paths mindful of cost: Chronos inference is expensive and should not run unnecessarily.
- Preserve the historical + forecast output contract used by `app.py` (`plot.parquet`, quantile columns `0.1/0.3/0.5/0.7/0.9`, plus `predictions`).
- Be careful with DB path assumptions: `src/database.py` writes to `gs://.../prices.db` by default, while `src/prediction.py` reads local `prices.db`.
- In `src/pricing.py`, `pricing_fetcher()` currently returns once instead of yielding continuously; maintain or fix intentionally and update callers/tests in the same change.

## Key Files To Read First
- Ingestion issues: `src/pricing.py`, `main.py`
- Storage/schema issues: `src/database.py`
- Forecast logic/performance: `src/prediction.py`
- Dashboard behavior: `app.py`
- Historical ingestion: `src/historic.py`
- Project context and run notes: `README.md`
