# Energy Pricing Journalist

Two-part system for collecting and visualizing MISO real-time energy prices.

## Architecture

### collector/
Python service that fetches MISO RT LMP data and stores it. On GCP it runs as a
**Cloud Run Job** triggered by Cloud Scheduler every 5 minutes. Locally it loops.

Outputs written to a GCS bucket (GCP) or `./data/` (local):
- `prices.parquet` — rolling 2-week history of all node prices
- `latest.json` — last 24h of HUB prices for the frontend

### frontend/
Single `index.html` with a **WebGPU** line chart. Deployed as a static page in GCS.
Fetches `latest.json` and re-fetches every 5 minutes automatically.

## Local Development

**Prerequisites:** Docker, Docker Compose

```bash
docker compose up
```

- Frontend: http://localhost:8080
- Raw data files: `./data/`

The collector runs immediately and then every 5 minutes. The web server starts
serving right away; the chart will appear once the first fetch completes (~10s).

## GCP Deployment

### Environment variables (collector)
| Variable | Description |
|---|---|
| `GCS_BUCKET` | GCS bucket name (enables GCS mode) |
| `LOCAL_DEV` | `true` to loop instead of run-once |
| `DATA_DIR` | Local output dir (default `/data`) |

### Build and push the collector
```bash
docker build -t gcr.io/PROJECT/miso-collector ./collector
docker push gcr.io/PROJECT/miso-collector
```

### Create the Cloud Run Job
```bash
gcloud run jobs create miso-collector \
  --image gcr.io/PROJECT/miso-collector \
  --set-env-vars GCS_BUCKET=YOUR_BUCKET \
  --region us-central1
```

### Schedule via Cloud Scheduler (every 5 minutes)
```bash
gcloud scheduler jobs create http miso-collector-trigger \
  --schedule "*/5 * * * *" \
  --uri "https://us-central1-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/PROJECT/jobs/miso-collector:run" \
  --http-method POST \
  --oauth-service-account-email YOUR_SA@PROJECT.iam.gserviceaccount.com
```

### Deploy the frontend to GCS
```bash
# Upload
gsutil cp frontend/index.html gs://YOUR_BUCKET/index.html

# Enable static website hosting
gsutil web set -m index.html gs://YOUR_BUCKET

# Make bucket public
gsutil iam ch allUsers:objectViewer gs://YOUR_BUCKET

# Set CORS on the bucket so the frontend can fetch latest.json
gsutil cors set - gs://YOUR_BUCKET <<'EOF'
[{"origin": ["*"], "method": ["GET"], "maxAgeSeconds": 60}]
EOF
```

> **Update `DATA_URL` in `frontend/index.html`** before uploading:
> ```js
> const DATA_URL = 'https://storage.googleapis.com/YOUR_BUCKET/latest.json';
> ```

## Data format

### prices.parquet columns
| Column | Type | Description |
|---|---|---|
| `location` | String | MISO node name |
| `lmp` | Float32 | Locational Marginal Price ($/MWh) |
| `mcc` | Float32 | Congestion component |
| `mlc` | Float32 | Loss component |
| `datetime` | Datetime (America/Chicago) | Snapshot time |

### latest.json schema
```json
{
  "updated": "<ISO 8601 timestamp>",
  "locations": ["LOC1", "LOC2"],
  "series": {
    "LOC1": [[unix_seconds, lmp], ...],
    "LOC2": [[unix_seconds, lmp], ...]
  }
}
```
Up to 20 HUB locations, up to 24 hours of data per location.
