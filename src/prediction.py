import polars as pl
import duckdb
from chronos import Chronos2Pipeline
import torch
import pandas as pd


def __snap_and_interpolate(df: pl.DataFrame) -> pl.DataFrame:
    # Step 1: Snap datetime to nearest 5-minute interval
    df_snapped = df.with_columns(
        pl.col("datetime").dt.round("5m").alias("datetime_snapped")
    )

    # Group by location and datetime_snapped, average if multiple values
    df_agg = (
        df_snapped.group_by(["location", "datetime_snapped"])
        .agg(pl.col("lmp").mean())
        .rename({"datetime_snapped": "datetime"})
    )

    # Step 2: Create complete 5-minute interval range for each location
    # Get min and max datetime across all data
    min_dt = df_agg.select(pl.col("datetime").min()).item()
    max_dt = df_agg.select(pl.col("datetime").max()).item()

    # Create complete datetime range at 5-minute intervals
    complete_range = pl.datetime_range(
        min_dt, max_dt, interval="5m", eager=True
    ).to_frame("datetime")

    # Get all unique locations
    locations = df_agg.select("location").unique()

    # Cross join to get all location-datetime combinations
    complete_grid = locations.join(complete_range, how="cross")

    # Step 3: Join with actual data and interpolate
    result = (
        complete_grid.join(df_agg, on=["location", "datetime"], how="left")
        .sort(["location", "datetime"])
        .with_columns(pl.col("lmp").interpolate().over("location"))
    )

    return result


def generate_predictions():
    accelerator = "cuda" if torch.cuda.is_available() else "cpu"
    pipeline = Chronos2Pipeline.from_pretrained(
        "amazon/chronos-2", device_map=accelerator
    )

    conn = duckdb.connect("prices.db")
    all_prices_pd = conn.sql("SELECT * FROM pricing_history").df()
    all_prices = pl.from_pandas(all_prices_pd)
    conn.close()

    # all_prices = pl.read_parquet("data.parquet").drop(["mlc", "mcc", "status_code"])
    all_prices = all_prices.with_columns(
        pl.col("datetime").dt.convert_time_zone("America/Chicago")
    )

    df = __snap_and_interpolate(all_prices)

    # Load historical target values and past values of covariates
    context_df = df.to_pandas()

    # Generate predictions with covariates
    pred_df = pipeline.predict_df(
        context_df,
        prediction_length=24,  # Number of steps to forecast
        quantile_levels=[
            0.1,
            0.3,
            0.5,
            0.7,
            0.9,
        ],  # Quantile for probabilistic forecast
        id_column="location",  # Column identifying different time series
        timestamp_column="datetime",  # Column with datetime information
        target="lmp",  # Column(s) with time series values to predict
    ).drop("target_name", axis=1)

    # TODO: future warning
    full_df = context_df.copy()
    for col in ["0.1", "0.3", "0.5", "0.7", "0.9", "predictions"]:
        full_df[col] = None
    pred_df["lmp"] = None

    dashboard_df = pd.concat([full_df, pred_df])

    # Find the latest datetime where predictions is NaN
    last_stamp = dashboard_df[dashboard_df["predictions"].isnull()]["datetime"].max()

    # If there is at least one NaN in predictions
    if pd.notna(last_stamp):
        for col in ["0.1", "0.3", "0.5", "0.7", "0.9", "predictions"]:
            # Fill the predictions value at that exact row with the corresponding lmp
            dashboard_df.loc[dashboard_df["datetime"] == last_stamp, col] = (
                dashboard_df.loc[dashboard_df["datetime"] == last_stamp, "lmp"]
            )

    return dashboard_df
