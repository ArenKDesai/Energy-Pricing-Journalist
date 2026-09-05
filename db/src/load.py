"""Load raw MISO LMP CSVs into a DuckDB database.

Source files are wide (one row per node/component, 24 hour-ending columns). This
reshapes them to one row per node-hour with lmp/mcc/mlc as columns.

Loading is done per market-month and is idempotent: re-running a month replaces
that month's rows, so an interrupted load can simply be re-run.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw"
DB = ROOT / "data" / "miso_lmp.duckdb"

HE_COLS = ", ".join(f'"HE {i}"' for i in range(1, 25))

SCHEMA = """
CREATE TABLE IF NOT EXISTS lmp (
    market     VARCHAR   NOT NULL,   -- 'DA' (day-ahead ex-post) or 'RT' (real-time)
    ts_est     TIMESTAMP NOT NULL,   -- hour-beginning, EST year-round (MISO does not shift for DST)
    he         SMALLINT  NOT NULL,   -- hour-ending 1..24, as published
    node       VARCHAR   NOT NULL,
    node_type  VARCHAR,              -- Hub / Loadzone / Interface / Gennode
    lmp        DOUBLE,
    mcc        DOUBLE,               -- marginal congestion component
    mlc        DOUBLE                -- marginal loss component
);
"""

# ts_est is hour-beginning: HE 1 covers 00:00-01:00, so ts = date + (he-1) hours.
LOAD_SQL = rf"""
INSERT INTO lmp
WITH src AS (
    SELECT
        "Node", "Type", "Value", {HE_COLS},
        strptime(regexp_extract(filename, '(\d{{8}})\.csv\.gz$', 1), '%Y%m%d') AS d
    FROM read_csv($glob, skip=4, header=true, all_varchar=true, filename=true,
                  union_by_name=true)
),
long AS (
    UNPIVOT src ON {HE_COLS} INTO NAME he_name VALUE price_txt
)
SELECT
    $market AS market,
    d + INTERVAL (CAST(regexp_extract(he_name, '(\d+)', 1) AS INTEGER) - 1) HOUR AS ts_est,
    CAST(regexp_extract(he_name, '(\d+)', 1) AS SMALLINT) AS he,
    "Node" AS node,
    any_value("Type") AS node_type,
    max(TRY_CAST(price_txt AS DOUBLE)) FILTER (WHERE "Value" = 'LMP') AS lmp,
    max(TRY_CAST(price_txt AS DOUBLE)) FILTER (WHERE "Value" = 'MCC') AS mcc,
    max(TRY_CAST(price_txt AS DOUBLE)) FILTER (WHERE "Value" = 'MLC') AS mlc
FROM long
GROUP BY market, ts_est, he, node
"""

VIEWS = """
CREATE OR REPLACE VIEW nodes AS
    SELECT node, any_value(node_type) AS node_type, count(*) AS obs,
           min(ts_est) AS first_ts, max(ts_est) AS last_ts
    FROM lmp GROUP BY node;

-- DA/RT side by side with the DA-RT spread (positive = DA priced above RT).
CREATE OR REPLACE VIEW da_rt_spread AS
    SELECT d.ts_est, d.node, d.node_type,
           d.lmp AS da_lmp, r.lmp AS rt_lmp, d.lmp - r.lmp AS da_minus_rt
    FROM (SELECT * FROM lmp WHERE market = 'DA') d
    JOIN (SELECT * FROM lmp WHERE market = 'RT') r
      ON d.ts_est = r.ts_est AND d.node = r.node;

-- Every node MISO tags as type 'Hub'. That is ~445 commercial pricing nodes,
-- NOT just the trading hubs — see major_hubs for those.
CREATE OR REPLACE VIEW hub_prices AS
    SELECT * FROM lmp WHERE node_type = 'Hub';

-- The eight major trading hubs, which is what the web app charts and what
-- export_history.py ships to the browser.
CREATE OR REPLACE VIEW major_hubs AS
    SELECT * FROM lmp WHERE node LIKE '%.HUB';
"""


def month_keys(market: str) -> list[tuple[str, str]]:
    """Return (year, month) pairs that have raw files for this market."""
    keys = set()
    for p in (RAW / market).rglob("*.csv.gz"):
        stem = p.stem  # YYYYMMDD.csv -> stem is YYYYMMDD.csv for .csv.gz
        stem = stem.replace(".csv", "")
        keys.add((stem[:4], stem[4:6]))
    return sorted(keys)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--markets", default="da,rt")
    ap.add_argument("--rebuild", action="store_true", help="drop and rebuild the table")
    args = ap.parse_args()

    con = duckdb.connect(str(DB))
    if args.rebuild:
        con.execute("DROP TABLE IF EXISTS lmp")
    con.execute(SCHEMA)

    for market in [m.strip() for m in args.markets.split(",") if m.strip()]:
        for year, month in month_keys(market):
            glob = str(RAW / market / year / f"{year}{month}*.csv.gz").replace("\\", "/")
            # Idempotent: clear anything previously loaded for this market-month.
            con.execute(
                "DELETE FROM lmp WHERE market = ? AND year(ts_est) = ? AND month(ts_est) = ?",
                [market.upper(), int(year), int(month)],
            )
            con.execute(LOAD_SQL, {"glob": glob, "market": market.upper()})
            n = con.execute(
                "SELECT count(*) FROM lmp WHERE market = ? AND year(ts_est) = ? AND month(ts_est) = ?",
                [market.upper(), int(year), int(month)],
            ).fetchone()[0]
            print(f"  {market.upper()} {year}-{month}: {n:,} rows", flush=True)

    con.execute(VIEWS)
    total = con.execute("SELECT count(*) FROM lmp").fetchone()[0]
    print(f"total rows: {total:,}")
    con.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
