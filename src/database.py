import duckdb
import polars as pl
from src.pricing import reload_prices_df

DATABASE_PATH = "prices.db"


def create_pricing_table(prices: pl.DataFrame) -> None:
    conn = duckdb.connect(database=DATABASE_PATH)
    conn.execute("CREATE TABLE pricing_history AS SELECT * FROM prices")
    conn.close()
