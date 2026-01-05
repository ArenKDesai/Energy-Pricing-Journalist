import duckdb
import polars as pl

DATABASE_PATH = "prices.db"


def update_database(prices: pl.DataFrame) -> None:
    conn = duckdb.connect(database=DATABASE_PATH)
    
    # Create table if it's the first time
    # NOTE: WHERE 1=0 creates the schema without uploading data automatically
    conn.execute("CREATE TABLE IF NOT EXISTS pricing_history AS SELECT * FROM prices WHERE 1=0")
    
    # Append the data
    conn.execute("INSERT INTO pricing_history SELECT * FROM prices")
    
    conn.close()