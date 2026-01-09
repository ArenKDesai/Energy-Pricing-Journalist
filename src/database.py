import duckdb
import polars as pl

DATABASE_PATH = "prices.db"


def update_database(prices: pl.DataFrame) -> None:
    conn = duckdb.connect(database=DATABASE_PATH)

    # Create table schema if it doesn't exist
    # NOTE: LIMIT 0 allows schema creation without filling data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pricing_history AS SELECT * FROM prices LIMIT 0"
    )

    # Append the data
    conn.execute("INSERT INTO pricing_history SELECT * FROM prices")

    conn.close()
