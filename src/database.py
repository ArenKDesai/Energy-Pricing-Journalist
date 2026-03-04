import duckdb
import polars as pl

DATABASE_PATH = "gs://realtime-energy-prices/prices.db"


def update_database(prices: pl.DataFrame) -> None:
    conn = duckdb.connect(database=DATABASE_PATH)

    # Connect to GCP
    conn.execute("INSTALL httpfs; LOAD httpfs;")
    conn.execute(f"ATTACH {DATABASE_PATH} AS cloud_db")

    # Create table schema if it doesn't exist
    # NOTE: LIMIT 0 allows schema creation without filling data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cloud_db.pricing_history AS SELECT * FROM prices LIMIT 0"
    )

    # Append the data
    conn.execute("INSERT INTO cloud_db.pricing_history SELECT * FROM prices")

    conn.close()
