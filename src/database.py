import duckdb
import polars as pl
import os

DATABASE_PATH = os.getenv("DATABASE_PATH", "gs://realtime-energy-prices/prices.db")


def _connect_and_attach() -> duckdb.DuckDBPyConnection:
    """
    Open a DuckDB connection and attach the configured database path as `prices_db`.
    """
    conn = duckdb.connect()
    if DATABASE_PATH.startswith("gs://"):
        conn.execute("INSTALL httpfs; LOAD httpfs;")

    conn.execute(f"ATTACH '{DATABASE_PATH}' AS prices_db")
    return conn


def update_database(prices: pl.DataFrame) -> None:
    conn = _connect_and_attach()
    conn.register("prices_df", prices.to_arrow())

    # Create table schema if it doesn't exist
    # NOTE: LIMIT 0 allows schema creation without filling data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS prices_db.pricing_history AS "
        "SELECT * FROM prices_df LIMIT 0"
    )

    # Append the data
    conn.execute("INSERT INTO prices_db.pricing_history SELECT * FROM prices_df")
    conn.unregister("prices_df")

    conn.close()
