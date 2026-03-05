import requests
import polars as pl
from datetime import datetime, timezone
import time
from typing import Iterator


TIME_BETWEEN_FETCHES = 60  # 1 minute
TOTAL_RETRIES = 3
RETRY_BACKOFF_SECONDS = [5, 60, 180]


def __get_prices(hub: bool) -> pl.DataFrame:
    """
    Helper function to get current prices.

    :param hub: Whether to fetch HUBs or GENs/INTs/LZNs
    :type hub: bool
    :return: Current price records.
    :rtype: DataFrame
    """
    # Request
    url = "https://api.misoenergy.org/MISORTWDDataBroker/DataBrokerServices.asmx"
    headers = {
        "Content-Type": "text/xml; charset=utf-8",
        "SOAPAction": "http://tempuri.org/MethodName",
    }
    if hub:
        payload = r'{"messageType":"GetDataByNodeTypes","clientMessage":{"nodeTypes":["HUB"]}}'
    else:
        payload = r'{"messageType":"GetDataByNodeTypes","clientMessage":{"nodeTypes":["GEN","INT","LZN"]}}'

    # Response
    # NOTE: letting an exception be raised here - handling logic is in reload_prices_df
    response = requests.post(url, headers=headers, data=payload, timeout=10)
    response.raise_for_status()

    # Fix column naming and selection
    df = pl.DataFrame(response.json()["data"])
    if "NSI" in df.columns:
        df = df.drop("NSI")
    df = df.rename({col: col.lower() for col in df.columns})

    # Fix dtypes
    value_cols = ["lmp", "mcc", "mlc"]
    df = df.select(["location"] + value_cols)
    for col in value_cols:
        df = df.with_columns(pl.col(col).cast(pl.Float32))

    # Metadata
    df = df.with_columns(
        pl.lit(response.status_code).cast(pl.Int16).alias("status_code")
    )
    # NOTE: since HUBs and other nodes are grabbed separately, the DTs might be slightly off.
    now_utc = datetime.now(timezone.utc)

    df = df.with_columns(
        pl.lit(now_utc).dt.convert_time_zone("America/Chicago").alias("datetime")
    )

    return df


def reload_prices_df() -> pl.DataFrame:
    """
    Get current prices record from MISO's LMP contour map.

    :return: Prices dataframe.
    :rtype: DataFrame
    """
    nodes = __get_prices(hub=False)
    # NOTE: apparently the HUBs are returned without specifying them in the request.
    # hubs = __get_prices(hub=True)
    # prices = pl.concat([nodes, hubs]).sort(by="location")

    return nodes


def fetch_prices_with_retry(total_retries: int = TOTAL_RETRIES) -> pl.DataFrame:
    """
    Fetch a single RT LMP snapshot with retry/backoff.

    :param total_retries: Maximum retry attempts after the first failed call.
    :type total_retries: int
    :return: Prices dataframe.
    :rtype: pl.DataFrame
    """
    attempts = 0
    while True:
        try:
            return reload_prices_df()
        except Exception as e:
            attempts += 1
            if attempts > total_retries:
                raise RuntimeError(
                    f"Failed to fetch prices after {total_retries} retries"
                ) from e

            sleep_seconds = RETRY_BACKOFF_SECONDS[min(attempts - 1, len(RETRY_BACKOFF_SECONDS) - 1)]
            print(f"Exception raised: {e}; retrying in {sleep_seconds}s (attempt {attempts}/{total_retries})")
            time.sleep(sleep_seconds)


def pricing_fetcher() -> Iterator[pl.DataFrame]:
    """
    Fetches RT LMP data forever.

    :return: Current RT LMP records to be downloaded and stored.
    :rtype: Iterator[DataFrame]
    """
    while True:
        yield fetch_prices_with_retry(TOTAL_RETRIES)
        time.sleep(TIME_BETWEEN_FETCHES)
