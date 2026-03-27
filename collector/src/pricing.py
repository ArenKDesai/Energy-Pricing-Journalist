import time
import requests
import polars as pl
from datetime import datetime, timezone

TOTAL_RETRIES = 3
RETRY_BACKOFF_SECONDS = [5, 60, 180]


def _fetch_nodes() -> pl.DataFrame:
    url = "https://api.misoenergy.org/MISORTWDDataBroker/DataBrokerServices.asmx"
    headers = {
        "Content-Type": "text/xml; charset=utf-8",
        "SOAPAction": "http://tempuri.org/MethodName",
    }
    payload = r'{"messageType":"GetDataByNodeTypes","clientMessage":{"nodeTypes":["GEN","INT","LZN"]}}'

    response = requests.post(url, headers=headers, data=payload, timeout=10)
    response.raise_for_status()

    df = pl.DataFrame(response.json()["data"])
    if "NSI" in df.columns:
        df = df.drop("NSI")
    df = df.rename({col: col.lower() for col in df.columns})

    value_cols = ["lmp", "mcc", "mlc"]
    df = df.select(["location"] + value_cols)
    for col in value_cols:
        df = df.with_columns(pl.col(col).cast(pl.Float32))

    now_utc = datetime.now(timezone.utc)
    df = df.with_columns(
        pl.lit(now_utc).dt.convert_time_zone("America/Chicago").alias("datetime")
    )

    return df


def fetch_prices_with_retry(total_retries: int = TOTAL_RETRIES) -> pl.DataFrame:
    attempts = 0
    while True:
        try:
            return _fetch_nodes()
        except Exception as e:
            attempts += 1
            if attempts > total_retries:
                raise RuntimeError(
                    f"Failed to fetch prices after {total_retries} retries"
                ) from e
            sleep_seconds = RETRY_BACKOFF_SECONDS[
                min(attempts - 1, len(RETRY_BACKOFF_SECONDS) - 1)
            ]
            print(
                f"Fetch error: {e}; retrying in {sleep_seconds}s ({attempts}/{total_retries})",
                flush=True,
            )
            time.sleep(sleep_seconds)
