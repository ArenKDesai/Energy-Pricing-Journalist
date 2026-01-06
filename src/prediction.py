import pandas as pd  # requires: pip install 'pandas[pyarrow]'
from chronos import Chronos2Pipeline
import polars as pl
import torch

all_prices = pl.read_csv("data.csv").drop(["mlc", "mcc"])
accelerator = "cuda" if torch.cuda.is_available else "cpu"
pipeline = Chronos2Pipeline.from_pretrained("amazon/chronos-2", device_map=accelerator)

# Load historical target values and past values of covariates
context_df = all_prices.to_pandas()

# Generate predictions with covariates
pred_df = pipeline.predict_df(
    context_df,
    prediction_length=12,  # Number of steps to forecast
    quantile_levels=[0.1, 0.5, 0.9],  # Quantile for probabilistic forecast
    id_column="location",  # Column identifying different time series
    timestamp_column="datetime",  # Column with datetime information
    target="lmp",  # Column(s) with time series values to predict
)

print(pred_df)