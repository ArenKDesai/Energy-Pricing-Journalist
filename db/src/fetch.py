"""Download MISO daily nodal LMP reports (DA ex-post, RT final/prelim).

Files are stored gzipped at data/raw/<market>/<year>/<YYYYMMDD>.csv.gz.
Re-running is safe: existing files are skipped, so an interrupted pull resumes.
"""
from __future__ import annotations

import argparse
import gzip
import json
import sys
import threading
import time
import random
from concurrent.futures import ThreadPoolExecutor
from datetime import date, timedelta
from pathlib import Path

import httpx

BASE = "https://docs.misoenergy.org/marketreports"
ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw"

# Earliest date MISO still publishes nodal LMP reports for (verified by probing).
FIRST_DAY = date(2023, 1, 1)

# Per market, the report variants to try in order of preference. RT settles a few
# days late, so recent days only exist as "prelim" until the final report lands.
VARIANTS = {
    "da": ["da_expost_lmp"],
    "rt": ["rt_lmp_final", "rt_lmp_prelim"],
}

_print_lock = threading.Lock()


def out_path(market: str, day: date) -> Path:
    return RAW / market / str(day.year) / f"{day:%Y%m%d}.csv.gz"


def daterange(start: date, end: date):
    d = start
    while d <= end:
        yield d
        d += timedelta(days=1)


def fetch_one(client: httpx.Client, market: str, day: date) -> dict:
    dest = out_path(market, day)
    if dest.exists() and dest.stat().st_size > 0:
        return {"market": market, "date": day.isoformat(), "status": "cached"}

    last_err = None
    for variant in VARIANTS[market]:
        url = f"{BASE}/{day:%Y%m%d}_{variant}.csv"
        for attempt in range(6):
            try:
                r = client.get(url)
            except httpx.HTTPError as e:  # transient network/DNS/TLS problem
                last_err = repr(e)
                # Back off before retrying; a dropped link otherwise burns every
                # attempt in milliseconds and the whole run reports false 404s.
                time.sleep(min(30.0, 2.0 ** attempt) + random.random())
                continue
            if r.status_code == 404:
                last_err = f"404 {variant}"
                break  # this variant genuinely does not exist; try the next one
            if r.status_code == 200 and r.content:
                # Guard against MISO serving an HTML error page with a 200.
                if not r.content.lstrip()[:1].isalpha():
                    last_err = "non-text body"
                    break
                dest.parent.mkdir(parents=True, exist_ok=True)
                tmp = dest.with_suffix(".tmp")
                with gzip.open(tmp, "wb") as fh:
                    fh.write(r.content)
                tmp.replace(dest)
                return {
                    "market": market,
                    "date": day.isoformat(),
                    "status": "ok",
                    "variant": variant,
                    "bytes": len(r.content),
                }
            last_err = f"HTTP {r.status_code}"
    return {"market": market, "date": day.isoformat(), "status": "missing", "error": last_err}


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--start", default=FIRST_DAY.isoformat())
    p.add_argument("--end", default=date.today().isoformat())
    p.add_argument("--markets", default="da,rt")
    p.add_argument("--workers", type=int, default=12)
    args = p.parse_args()

    start = date.fromisoformat(args.start)
    end = date.fromisoformat(args.end)
    markets = [m.strip() for m in args.markets.split(",") if m.strip()]

    jobs = [(m, d) for d in daterange(start, end) for m in markets]
    print(f"fetching {len(jobs)} day-market files ({start} .. {end})", flush=True)

    results: list[dict] = []
    limits = httpx.Limits(max_connections=args.workers)
    timeout = httpx.Timeout(60.0, connect=20.0)
    with httpx.Client(limits=limits, timeout=timeout, follow_redirects=True,
                      headers={"User-Agent": "miso-lmp-archive/1.0"}) as client:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            for i, res in enumerate(pool.map(lambda j: fetch_one(client, *j), jobs), 1):
                results.append(res)
                if i % 100 == 0 or i == len(jobs):
                    ok = sum(r["status"] == "ok" for r in results)
                    cached = sum(r["status"] == "cached" for r in results)
                    miss = sum(r["status"] == "missing" for r in results)
                    with _print_lock:
                        print(f"  {i}/{len(jobs)}  new={ok} cached={cached} missing={miss}", flush=True)

    manifest = ROOT / "data" / "fetch_manifest.json"
    manifest.write_text(json.dumps(results, indent=1))
    missing = [r for r in results if r["status"] == "missing"]
    print(f"done. new={sum(r['status']=='ok' for r in results)} "
          f"cached={sum(r['status']=='cached' for r in results)} missing={len(missing)}")
    for r in missing[:20]:
        print(f"  MISSING {r['market']} {r['date']}: {r.get('error')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
