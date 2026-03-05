from datetime import datetime
from dotenv import load_dotenv

# local
from src.pricing import fetch_prices_with_retry
from src.database import update_database

# Setup
load_dotenv()


def main():
    """
    Cloud Run Job entrypoint.

    Fetch one RT LMP snapshot, save it to DuckDB, and exit.
    """
    last_df = fetch_prices_with_retry()
    if last_df.is_empty():
        raise RuntimeError("Fetched an empty pricing dataframe; aborting database update")

    update_database(last_df)
    print(f"{datetime.now().ctime()}\tSaved {last_df.height} pricing rows")


if __name__ == "__main__":
    main()
