# Instructions
Run `docker compose up -d`.

You can view the data at `localhost:8050`. 

The duckdb persistent table will be stored in `prices.db`. 

## Plans
1. ~~Ingest RT data from LMP dashboard~~
2. ~~Build repository for that data (hold maybe 1 month of data, probably less)~~
3. ~~Build dashboard (plotly) to show highlights~~
4. Imporve forecasting, include metrics

### TODO:
- Fix TODOs
- Limit `plot.csv` for size and speed control (1 month?)

## Bug Fix Log
2026-01-05 19:35: Data was added to the duckdb twice due to duplicate data from the scraping requests. 

2026-01-06 20:19: Due to restarting the server and occasional maintenance, the RT LMP records have gaps where they weren't recorded. I'll use interpolation to fill these gaps so Chronos2 won't get tripped up. 