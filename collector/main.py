import os
import sys
import time
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo

CHICAGO_TZ = ZoneInfo("America/Chicago")

import polars as pl

from src.pricing import fetch_prices_with_retry
from src.storage import get_storage

LOCAL_DEV = os.getenv("LOCAL_DEV", "").lower() in ("1", "true", "yes")
INTERVAL_SECONDS = 300  # 5 minutes
TWO_WEEKS = timedelta(weeks=2)
ONE_DAY = timedelta(hours=24)
MAX_JSON_LOCATIONS = 20


def build_json(df: pl.DataFrame) -> dict:
    """Build the latest.json payload consumed by the frontend."""
    cutoff_24h = datetime.now(CHICAGO_TZ) - ONE_DAY
    recent = df.filter(pl.col("datetime") >= cutoff_24h)

    # Prefer HUB nodes; fall back to all locations
    hubs = recent.filter(pl.col("location").str.to_uppercase().str.contains("HUB"))
    source = hubs if not hubs.is_empty() else recent

    locations = sorted(source["location"].unique().to_list())[:MAX_JSON_LOCATIONS]

    series: dict[str, list] = {}
    for loc in locations:
        loc_df = (
            source.filter(pl.col("location") == loc)
            .sort("datetime")
            .drop_nulls(subset=["lmp"])
            .select(["datetime", "lmp"])
        )
        series[loc] = [
            [int(row["datetime"].timestamp()), round(float(row["lmp"]), 4)]
            for row in loc_df.to_dicts()
        ]

    return {
        "updated": datetime.now(timezone.utc).isoformat(),
        "locations": locations,
        "series": series,
    }


def run_once() -> None:
    print(f"{datetime.now().ctime()}\tFetching MISO RT LMP...", flush=True)
    new_data = fetch_prices_with_retry()

    if new_data.is_empty():
        raise RuntimeError("Fetched empty dataframe from MISO API")

    storage = get_storage()
    existing = storage.read_parquet()

    combined = pl.concat([existing, new_data]) if existing is not None else new_data

    cutoff = datetime.now(CHICAGO_TZ) - TWO_WEEKS
    combined = combined.filter(pl.col("datetime") >= cutoff)

    storage.write_parquet(combined)

    payload = build_json(combined)
    storage.write_json(payload)

    print(
        f"{datetime.now().ctime()}\tSaved {combined.height} rows; "
        f"{len(payload['locations'])} locations in JSON",
        flush=True,
    )


if __name__ == "__main__":
    if LOCAL_DEV:
        while True:
            try:
                run_once()
            except Exception as e:
                print(f"Error: {e}", file=sys.stderr, flush=True)
            print(f"Sleeping {INTERVAL_SECONDS}s...", flush=True)
            time.sleep(INTERVAL_SECONDS)
    else:
        run_once()
