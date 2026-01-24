# Energy Pricing Journalist: Real-Time MISO LMP Tracker

![Quant Finance](https://img.shields.io/badge/Domain-Quant_Finance-1182c3)
![Status](https://img.shields.io/badge/Status-Active-success)

A real-time energy market monitoring and forecasting system for MISO (Midcontinent Independent System Operator) Real-Time Locational Marginal Prices (LMP). This project demonstrates a full-stack quantitative development pipeline, traversing real-time data ingestion, probabilistic machine learning, and low-latency interactive visualization.

**Live Dashboard:** [energy-pricing-journalist.onrender.com](https://energy-pricing-journalist.onrender.com/)

## Key Features

### 🚀 High-Frequency Data Pipeline (`Polars` + `DuckDB`)
- **Real-Time Ingestion**: continuously scrapes live LMP contour maps to capture market ticks as they happen.
- **Efficient Storage**: Utilizes **DuckDB** for high-performance localized SQL querying and **Polars** for blazing fast in-memory data manipulation.
- **Data Engineering**: Data is normalized, snapped to 5-minute intervals, and stored in optimized Parquet format for cloud persistence.

### 🔮 Probabilistic Forecasting (`Chronos` / `PyTorch`)
- **State-of-the-Art Transformers**: Implements **Chronos-2** (by Amazon), a pretrained time-series foundation model based on language modeling architectures.
- **Quantile Predictions**: Generates probabilistic forecasts (10%, 30%, 50%, 70%, 90% quantiles) to model market uncertainty and volatility, crucial for risk-adjusted trading strategies.
- **GPU Acceleration**: Leveraging PyTorch for inference, optimized for CUDA environments when available.

### 📊 Interactive Financial Dashboard (`Dash` + `Plotly`)
- **Responsive UI**: Built with **Dash** and **Plotly** to provide deep interactivity, allowing users to zoom, pan, and inspect granular price movements.
- **Visual Analytics**: Visualizes historical price action alongside projected uncertainty bands, aiding in rapid decision-making.

## Technical Architecture

The system operates on a hybrid architecture to ensure reliability and speed:

1.  **Backend (Data & ML Layer)**:
    *   A persistent worker loop fetches live market data.
    *   New data triggers an update to the local DuckDB instance.
    *   The **Chronos** model generates fresh forecasts upon significant data updates.
    *   Processed data (historical + forecast) is exported to an S3-compatible object storage (Cloudflare R2).

2.  **Frontend (Presentation Layer)**:
    *   A lightweight Dash application reads the latest `parquet` file directly from the object store.
    *   This decoupling allows the heavy ML inference to run independently of the user-facing dashboard, ensuring zero latency for end-users.

## Tech Stack

*   **Languages**: Python 3.10+
*   **Data Manipulation**: Polars, Pandas
*   **Database**: DuckDB, Parquet
*   **Machine Learning**: PyTorch, HuggingFace Transformers (Chronos)
*   **Visualization**: Plotly, Dash
*   **Infrastructure**: Docker, Docker Compose, Cloudflare R2

## Getting Started

To run the full stack locally (requires Docker):

```bash
docker compose up -d
```

- **Dashboard**: Access visualization at `http://localhost:8050`
- **Backend logs**: view extraction and inference progress via `docker compose logs -f`

## Project Roadmap

*   [x] **Real-time Data Ingestion**: Robust scraping and cleaning pipeline.
*   [x] **Storage Layer**: Implementation of DuckDB and Parquet archiving.
*   [x] **Forecasting Engine**: Integration of Chronos-2 for probabilistic inference.
*   [x] **Dashboard**: Interactive Plotly web app.
*   [ ] **Backtesting Framework**: Evaluate forecast performance against realized LMPs.
*   [ ] **Signal Generation**: Automated alerts for price spikes or arbitrage opportunities.