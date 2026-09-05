-- Example queries. Run with:
--   .venv/Scripts/python.exe -c "import duckdb;con=duckdb.connect('data/miso_lmp.duckdb');print(con.sql(open('src/queries.sql').read().split(';')[0]))"

-- Monthly average DA vs RT at the major hubs
SELECT date_trunc('month', ts_est) AS month, node,
       avg(da_lmp) AS da, avg(rt_lmp) AS rt, avg(da_minus_rt) AS spread
FROM da_rt_spread
WHERE node LIKE '%.HUB'
GROUP BY 1, 2
ORDER BY 1, 2;

-- Most congested nodes by absolute congestion component
SELECT node, node_type, avg(abs(mcc)) AS avg_abs_mcc, count(*) AS hours
FROM lmp WHERE market = 'RT'
GROUP BY 1, 2 HAVING count(*) > 1000
ORDER BY avg_abs_mcc DESC LIMIT 25;

-- Hourly shape: average price by hour-ending, DA vs RT
SELECT he,
       avg(lmp) FILTER (WHERE market = 'DA') AS da,
       avg(lmp) FILTER (WHERE market = 'RT') AS rt
FROM lmp WHERE node = 'INDIANA.HUB'
GROUP BY he ORDER BY he;

-- Negative price hours per year (renewable oversupply / congestion)
SELECT year(ts_est) AS yr, market,
       count(*) FILTER (WHERE lmp < 0) AS neg_hours,
       100.0 * count(*) FILTER (WHERE lmp < 0) / count(*) AS pct_neg
FROM lmp GROUP BY 1, 2 ORDER BY 1, 2;
