import polars as pl
import requests
from datetime import datetime
import os


BASE_MISOREPORT = "https://docs.misoenergy.org/marketreports/"


def get_zip(dt: datetime) -> str:
    dt_str = dt.strftime("%Y%m%d")
    download_str = f"{dt_str}_5min_exante_lmp.xlsx"
    res = requests.get(f"{BASE_MISOREPORT}{download_str}")
    with open(download_str, "wb") as f:
        f.write(res.content)

    return download_str


def process_xcl(download_str: str, rmv: bool = True):
    df = pl.read_excel(download_str, read_options={"header_row": 3})
    if rmv:
        os.remove(download_str)

    # cull the data to save space
    df = (
        df.rename(
            {"Time (EST)": "timestamp", "CP Node": "node", "RT Ex-Ante LMP": "rt_lmp"}
        )
        .select(["timestamp", "node", "rt_lmp"])
        .with_columns(pl.col("timestamp").dt.replace_time_zone("EST"))
        .with_columns(pl.col("timestamp").dt.convert_time_zone("America/Chicago"))
    )
    return df


def historic_lmp_wrapper(dt: datetime):
    dstr = get_zip(dt)
    df = process_xcl(dstr)
    return df

if __name__ == "__main__":
    dt = datetime(2026, 3, 1)
    dstr = get_zip(dt)
    df = process_xcl(dstr)
    print(df)