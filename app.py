import dash
from dash import dcc, html, Input, Output, State
import plotly.graph_objects as go
import pandas as pd
import io
import requests

# Initialize the Dash app
app = dash.Dash(__name__)


DATA_URL = "https://pub-64bb320981ca4bebbdb6ef3c42db701b.r2.dev/plot.parquet"

def get_latest_data():
    try:
        # We use requests to get around potential header issues
        response = requests.get(DATA_URL)
        
        if response.status_code == 200:
            # Use BytesIO to turn the raw content into a file-like object for pandas
            return pd.read_parquet(io.BytesIO(response.content))
        else:
            print(f"Error: Received status code {response.status_code}")
            return None
    except Exception as e:
        print(f"Fetch failed: {e}")
        return None


# Define a function to load data and generate layout (runs on every page load)
def serve_layout():
    # Reload the CSV fresh every time the page is accessed
    dashboard_df = get_latest_data()
    dashboard_df["datetime"] = pd.to_datetime(dashboard_df["datetime"])

    # Get unique locations (will be up-to-date with new data)
    locations = dashboard_df["location"].unique()

    return html.Div(
        style={
            "backgroundColor": "#0d1117",
            "color": "#c9d1d9",
            "fontFamily": "Arial, sans-serif",
            "padding": "20px",
        },
        children=[
            html.H1(
                "MISO Market LMP Dashboard",
                style={"textAlign": "center", "color": "#AAF0C1"},
            ),
            html.Div(
                [
                    html.Label(
                        "Select Location:",
                        style={"fontSize": "18px", "marginRight": "10px"},
                    ),
                    dcc.Dropdown(
                        id="location-dropdown",
                        options=[{"label": loc, "value": loc} for loc in locations],
                        value=locations[0] if len(locations) > 0 else None,
                        style={
                            "width": "300px",
                            "backgroundColor": "#161b22",
                            "color": "#c9d1d9",
                            "border": "1px solid #30363d",
                        },
                    ),
                ],
                style={
                    "display": "flex",
                    "justifyContent": "center",
                    "alignItems": "center",
                    "marginBottom": "20px",
                },
            ),
            dcc.Graph(id="lmp-graph", style={"height": "600px"}),
            # Hidden interval for auto-refresh every 4 minutes (240000 ms)
            dcc.Interval(
                id="auto-refresh-interval",
                interval=4 * 60 * 1000,  # 4 minutes
                n_intervals=0,
                max_intervals=-1,  # Run indefinitely
            ),
        ],
    )


# Assign the dynamic layout
app.layout = serve_layout


# Callback to update the graph (triggered by dropdown OR interval)
@app.callback(
    Output("lmp-graph", "figure"),
    Input("location-dropdown", "value"),
    Input("auto-refresh-interval", "n_intervals"),
    State("location-dropdown", "value"),  # To keep current selection after interval
)
def update_graph(selected_location, n_intervals, current_location):
    # Use current selection if None (helps after interval trigger)
    if selected_location is None:
        selected_location = current_location

    if selected_location is None:
        return go.Figure()  # Empty figure if no location

    # Re-load the CSV here too for the most up-to-date data during auto-refreshes
    dashboard_df = pd.read_csv("plot.parquet")
    dashboard_df["datetime"] = pd.to_datetime(dashboard_df["datetime"])

    # Filter data for the selected location
    filtered_df = dashboard_df[
        dashboard_df["location"] == selected_location
    ].sort_values("datetime")

    # Calculate initial time range: last 24 hours based on max datetime
    max_dt = filtered_df["datetime"].max()
    min_dt = max_dt - pd.Timedelta(hours=24)

    # Separate historical (actual LMP) and future (predictions)
    historical = filtered_df[filtered_df["lmp"].notna()]
    predictions = filtered_df[filtered_df["predictions"].notna()]

    # Create the figure (same as your original)
    fig = go.Figure()

    fig.add_trace(
        go.Scatter(
            x=historical["datetime"],
            y=historical["lmp"],
            mode="lines",
            name="Actual LMP",
            line=dict(color="#AAF0C1", width=2),
        )
    )

    if not predictions.empty:
        # Outer band (0.1 - 0.9)
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["0.9"],
                mode="lines",
                line=dict(color="rgba(143, 201, 163, 0.2)", width=0),
                showlegend=False,
            )
        )
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["0.1"],
                mode="lines",
                fill="tonexty",
                fillcolor="rgba(143, 201, 163, 0.2)",
                line=dict(color="rgba(143, 201, 163, 0.2)", width=0),
                name="0.1 - 0.9 Band",
            )
        )

        # Inner band (0.3 - 0.7)
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["0.7"],
                mode="lines",
                line=dict(color="rgba(143, 201, 163, 0.4)", width=0),
                showlegend=False,
            )
        )
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["0.3"],
                mode="lines",
                fill="tonexty",
                fillcolor="rgba(143, 201, 163, 0.4)",
                line=dict(color="rgba(143, 201, 163, 0.4)", width=0),
                name="0.3 - 0.7 Band",
            )
        )

        # Median prediction
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["predictions"],
                mode="lines",
                name="Median Prediction",
                line=dict(color="#ffffff", width=2, dash="dot"),
            )
        )

    # Layout (same as original)
    fig.update_layout(
        title=f"RealTime LMP Data for {selected_location}",
        xaxis_title="Datetime",
        yaxis_title="Price ($/MWh)",
        template="plotly_dark",
        plot_bgcolor="#0d1117",
        paper_bgcolor="#0d1117",
        font=dict(color="#c9d1d9"),
        xaxis=dict(
            rangeselector=dict(
                buttons=list(
                    [
                        dict(count=6, label="6h", step="hour", stepmode="backward"),
                        dict(count=1, label="1d", step="day", stepmode="backward"),
                        dict(count=7, label="1w", step="day", stepmode="backward"),
                        dict(step="all"),
                    ]
                ),
                bgcolor="#161b22",
                activecolor="#30363d",
                bordercolor="#30363d",
            ),
            rangeslider=dict(visible=True, bgcolor="#161b22", bordercolor="#30363d"),
            type="date",
            range=[min_dt, max_dt],
        ),
        legend=dict(orientation="h", yanchor="bottom", y=1.02, xanchor="right", x=1),
        margin=dict(l=40, r=40, t=40, b=40),
        hovermode="x unified",
    )

    return fig


# Run the app
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8050, debug=True)
