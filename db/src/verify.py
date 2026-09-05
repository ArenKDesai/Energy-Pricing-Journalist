"""Coverage and sanity checks on the built database."""
from __future__ import annotations

import sys
from pathlib import Path

import duckdb

DB = Path(__file__).resolve().parent.parent / "data" / "miso_lmp.duckdb"


def main() -> int:
    con = duckdb.connect(str(DB), read_only=True)
    q = lambda s: con.execute(s).fetchall()

    print("== row counts by market ==")
    for m, n, d0, d1 in q("""SELECT market, count(*), min(ts_est), max(ts_est)
                             FROM lmp GROUP BY market ORDER BY market"""):
        print(f"  {m}: {n:,} rows   {d0}  ..  {d1}")

    print("\n== calendar gaps (days with no data) ==")
    gaps = q("""
        WITH cal AS (
            SELECT unnest(generate_series(
                (SELECT min(ts_est)::DATE FROM lmp),
                (SELECT max(ts_est)::DATE FROM lmp),
                INTERVAL 1 DAY))::DATE AS d
        ), have AS (SELECT DISTINCT market, ts_est::DATE AS d FROM lmp)
        SELECT m.market, cal.d FROM cal
        CROSS JOIN (SELECT DISTINCT market FROM lmp) m
        LEFT JOIN have ON have.d = cal.d AND have.market = m.market
        WHERE have.d IS NULL ORDER BY m.market, cal.d
    """)
    print(f"  {len(gaps)} missing market-days" + (":" if gaps else ""))
    for market, d in gaps[:40]:
        print(f"    {market} {d}")

    print("\n== days not having exactly 24 hours ==")
    bad = q("""SELECT market, ts_est::DATE AS d, count(DISTINCT he) AS hrs
               FROM lmp GROUP BY 1,2 HAVING count(DISTINCT he) <> 24
               ORDER BY 1,2""")
    print(f"  {len(bad)} such days")
    for r in bad[:20]:
        print(f"    {r[0]} {r[1]}: {r[2]} hours")

    print("\n== node count by year ==")
    for yr, m, n in q("""SELECT year(ts_est), market, count(DISTINCT node)
                         FROM lmp GROUP BY 1,2 ORDER BY 1,2"""):
        print(f"  {yr} {m}: {n:,} nodes")

    print("\n== price sanity ==")
    for label, sql in [
        ("null LMPs", "SELECT count(*) FROM lmp WHERE lmp IS NULL"),
        ("|LMP| > $10,000", "SELECT count(*) FROM lmp WHERE abs(lmp) > 10000"),
        ("negative LMPs", "SELECT count(*) FROM lmp WHERE lmp < 0"),
    ]:
        print(f"  {label}: {q(sql)[0][0]:,}")

    print("\n  min/median/max LMP by market:")
    for m, lo, med, hi in q("""SELECT market, min(lmp), median(lmp), max(lmp)
                               FROM lmp GROUP BY market ORDER BY market"""):
        print(f"    {m}: {lo:,.2f} / {med:,.2f} / {hi:,.2f}")

    con.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
