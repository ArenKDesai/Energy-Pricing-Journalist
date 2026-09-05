"""Export hub LMP history from DuckDB into the static file the web app loads.

The web app is browser-only: MISO's market-report host sends no CORS headers, so
the page cannot read history itself. This writes the history it needs as a small
same-origin asset that ships with the built site.

Output is column-oriented on an implicit hourly grid — the timestamps are
`t0 + i * interval_seconds`, so only the prices are stored. 30 days of DA+RT for
8 hubs is ~11k numbers (~90 KB, ~20 KB gzipped over the wire).

    db/.venv/Scripts/python.exe db/src/export_history.py --days 30
"""
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent          # db/
REPO = ROOT.parent                                     # repo root
DB = ROOT / "data" / "miso_lmp.duckdb"
OUT = REPO / "web" / "assets" / "hub_history.json"

# The eight major MISO trading hubs — the same set the live DataBroker feed is
# filtered to in web/src/types.rs. Matching on node_type = 'Hub' would be wrong:
# MISO tags ~445 commercial pricing nodes with that type.
HUBS = [
    "ARKANSAS.HUB",
    "ILLINOIS.HUB",
    "INDIANA.HUB",
    "LOUISIANA.HUB",
    "MICHIGAN.HUB",
    "MINN.HUB",
    "MS.HUB",
    "TEXAS.HUB",
]

# Fixed constant list, safe to inline into SQL.
HUB_SQL = "(" + ", ".join(f"'{h}'" for h in HUBS) + ")"

HOUR = 3600
# MISO publishes these reports in EST year-round and never shifts for DST, so
# `ts_est` is a naive local-EST timestamp. DuckDB's epoch() reads a naive
# TIMESTAMP as if it were UTC, so undo the fixed UTC-5 offset by adding it back.
EST_OFFSET = 5 * HOUR


def export(days: int, db_path: Path, out_path: Path) -> dict:
    con = duckdb.connect(str(db_path), read_only=True)

    # Anchor the window on the newest RT hour. RT settles a few days late, so it
    # always trails DA; anchoring on RT keeps the two markets aligned instead of
    # emitting a run of trailing nulls for RT.
    row = con.execute(
        """
        SELECT max(ts_est) FROM lmp
        WHERE market = 'RT' AND node IN {hub_list}
        """.format(hub_list=HUB_SQL)
    ).fetchone()
    if not row or row[0] is None:
        raise SystemExit("no RT hub rows in the database — run fetch.py and load.py first")

    end = row[0]
    n = days * 24
    start = end - timedelta(hours=n - 1)

    rows = con.execute(
        """
        SELECT market, node, epoch(ts_est)::BIGINT AS e, lmp
        FROM lmp
        WHERE node IN {hub_list}
          AND ts_est BETWEEN ? AND ?
          AND lmp IS NOT NULL
        """.format(hub_list=HUB_SQL),
        [start, end],
    ).fetchall()

    t0 = int(start.replace(tzinfo=timezone.utc).timestamp()) + EST_OFFSET

    # market -> hub -> dense list of `n` prices, null where the hour is missing.
    series: dict[str, dict[str, list]] = {
        m: {h: [None] * n for h in HUBS} for m in ("DA", "RT")
    }
    for market, node, e, lmp in rows:
        i = (int(e) + EST_OFFSET - t0) // HOUR
        if 0 <= i < n:
            series[market][node][i] = round(float(lmp), 2)

    filled = {m: sum(v is not None for h in HUBS for v in series[m][h]) for m in series}
    rt_end = t0 + (n - 1) * HOUR

    payload = {
        "generated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "MISO daily market reports (DA ex-post / RT final) via db/ pipeline",
        "interval_seconds": HOUR,
        "t0": t0,
        "n": n,
        "rt_end": rt_end,
        "hubs": HUBS,
        "da": series["DA"],
        "rt": series["RT"],
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, separators=(",", ":")))
    con.close()

    return {
        "hours": n,
        "t0": t0,
        "rt_end": rt_end,
        "filled": filled,
        "bytes": out_path.stat().st_size,
        "path": out_path,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--days", type=int, default=30,
                    help="hours of history = days * 24 (default 30)")
    ap.add_argument("--db", type=Path, default=DB)
    ap.add_argument("--out", type=Path, default=OUT)
    args = ap.parse_args()

    info = export(args.days, args.db, args.out)
    span = lambda e: datetime.fromtimestamp(e, timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    print(f"wrote {info['path']}")
    print(f"  {info['hours']} hourly steps   {span(info['t0'])}  ..  {span(info['rt_end'])}")
    for market, k in info["filled"].items():
        print(f"  {market}: {k:,} / {info['hours'] * len(HUBS):,} values")
    print(f"  {info['bytes']:,} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
