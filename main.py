from datetime import datetime, timedelta
import time

# local
from src.pricing import reload_prices_df, pricing_fetcher
from src.utils import records_equal
from src.database import update_database
from src.prediction import generate_predictions


break_flag = False


def redo_predictions():
    global break_flag
    if not break_flag:
        start = time.time()
        dashboard_df = generate_predictions()
        dashboard_df.to_csv("plot.csv")
        end = time.time()
        print(f"{datetime.now().ctime()}\tPrediction took {end - start:0.2f}s")
    else:
        print(f"{datetime.now().ctime()}\tWARNING: BREAK FLAG IS SET. NO PREDICTIONS")

    # If this takes 5 minutes or longer, stop it so we don't lose LMPs
    if (end - 60 * 5) <= start:
        break_flag = True


def main():
    # Get first prices df to start permanent fetching
    last_df = reload_prices_df()
    print(last_df.head())
    update_database(last_df)

    # Loop for new data
    fetcher = pricing_fetcher()
    for df in fetcher:
        if not records_equal(df, last_df):
            print(df.head())
            update_database(df)
            redo_predictions()
            last_df = df.clone()


if __name__ == "__main__":
    main()
