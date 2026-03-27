import io
import json
import os
from pathlib import Path
from typing import Optional

import polars as pl

BUCKET_NAME = os.getenv("GCS_BUCKET", "")
DATA_DIR = os.getenv("DATA_DIR", "/data")
PARQUET_FILENAME = "prices.parquet"
JSON_FILENAME = "latest.json"


class Storage:
    def read_parquet(self) -> Optional[pl.DataFrame]:
        raise NotImplementedError

    def write_parquet(self, df: pl.DataFrame) -> None:
        raise NotImplementedError

    def write_json(self, data: dict) -> None:
        raise NotImplementedError


class LocalStorage(Storage):
    def __init__(self):
        self.data_dir = Path(DATA_DIR)
        self.data_dir.mkdir(parents=True, exist_ok=True)

    def read_parquet(self) -> Optional[pl.DataFrame]:
        path = self.data_dir / PARQUET_FILENAME
        if not path.exists():
            return None
        return pl.read_parquet(path)

    def write_parquet(self, df: pl.DataFrame) -> None:
        df.write_parquet(self.data_dir / PARQUET_FILENAME)

    def write_json(self, data: dict) -> None:
        (self.data_dir / JSON_FILENAME).write_text(json.dumps(data))


class GCSStorage(Storage):
    def __init__(self):
        from google.cloud import storage  # noqa: PLC0415
        self.client = storage.Client()
        self.bucket = self.client.bucket(BUCKET_NAME)

    def read_parquet(self) -> Optional[pl.DataFrame]:
        blob = self.bucket.blob(PARQUET_FILENAME)
        if not blob.exists():
            return None
        return pl.read_parquet(io.BytesIO(blob.download_as_bytes()))

    def write_parquet(self, df: pl.DataFrame) -> None:
        buf = io.BytesIO()
        df.write_parquet(buf)
        buf.seek(0)
        blob = self.bucket.blob(PARQUET_FILENAME)
        blob.upload_from_file(buf, content_type="application/octet-stream")

    def write_json(self, data: dict) -> None:
        blob = self.bucket.blob(JSON_FILENAME)
        blob.upload_from_string(json.dumps(data), content_type="application/json")


def get_storage() -> Storage:
    if BUCKET_NAME:
        return GCSStorage()
    return LocalStorage()
