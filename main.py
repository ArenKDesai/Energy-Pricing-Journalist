from src.pricing import reload_prices_df, pricing_fetcher
from src.utils import records_equal
from src.database import update_database


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
            last_df = df.clone()


if __name__ == "__main__":
    main()
