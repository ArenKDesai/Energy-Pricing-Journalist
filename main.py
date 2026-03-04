from datetime import datetime, timedelta
import time
import boto3
from dotenv import load_dotenv
import polars as pl

# local
from src.pricing import reload_prices_df, pricing_fetcher
from src.utils import records_equal
from src.database import update_database
from src.prediction import generate_predictions

# Setup
load_dotenv()

# Global vars
break_flag = False


def upload_to_cloud():
    s3 = boto3.client(
        "s3",
        endpoint_url="https://dfb89c59c6fbfa229279b1dd9f18c54c.r2.cloudflarestorage.com",  # If using R2
        region_name="auto",
    )
    s3.upload_file("plot.parquet", "epj-parquet", "plot.parquet")


def redo_predictions() -> None:
    """
    Run Chronos2 for new predictions and generate `plot.csv`.
    """
    global break_flag

    if not break_flag:
        start = time.time()
        dashboard_df = generate_predictions()

        # limit to 2 weeks
        last_timestamp = dashboard_df["datetime"].max()
        two_weeks = last_timestamp - timedelta(days=14)
        dashboard_df = dashboard_df[dashboard_df["datetime"] >= two_weeks]

        dashboard_df.to_parquet("plot.parquet")
        # upload_to_cloud()  # Push the new data
        end = time.time()

        # If this takes 5 minutes or longer, stop it so we don't lose LMPs
        print(f"{datetime.now().ctime()}\tPrediction took {end - start:0.2f}s")
        if (end - 60 * 5) >= start:  # if it took longer than 5 minutes,
            break_flag = True  # stop so data pipeline doesn't lag.
    else:
        print(f"{datetime.now().ctime()}\tWARNING: BREAK FLAG IS SET. NO PREDICTIONS")


def main():
    """
    Main function. Scrape LMP contour map for data and generate table and predictions.
    """
    # Get first prices df to start permanent fetching
    last_df = reload_prices_df()
    update_database(last_df)

    # Loop for new data
    # df = pricing_fetcher()
    print(f"{datetime.now().ctime()}\tDuckdb update took {end - start:0.2f}s")


if __name__ == "__main__":
    main()
