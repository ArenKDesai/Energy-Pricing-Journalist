import dash
from dash import dcc, html
from dash.dependencies import Input, Output
import plotly.graph_objects as go
import pandas as pd

# Assume dashboard_df is your DataFrame
# For demonstration, if needed, you can load or create it here, but assuming it's provided
dashboard_df = pd.read_csv("plot.csv")  # Replace if necessary
dashboard_df["datetime"] = pd.to_datetime(dashboard_df["datetime"])

# Get unique locations
locations = dashboard_df["location"].unique()

# Initialize the Dash app
app = dash.Dash(__name__)

# Layout of the dashboard
app.layout = html.Div(
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
                    value=locations[0],  # Default to first location
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
    ],
)


# Callback to update the graph based on selected location
@app.callback(Output("lmp-graph", "figure"), [Input("location-dropdown", "value")])
def update_graph(selected_location):
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

    # Create the figure
    fig = go.Figure()

    # Add actual LMP line
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
        # Add outer band (0.1 - 0.9)
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

        # Add inner band (0.3 - 0.7)
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

        # Add median prediction line (using 'predictions' which is 0.5)
        fig.add_trace(
            go.Scatter(
                x=predictions["datetime"],
                y=predictions["predictions"],
                mode="lines",
                name="Median Prediction",
                line=dict(color="#ffffff", width=2, dash="dot"),
            )
        )

    # Update layout for sleek dark theme
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
                        dict(count=12, label="12h", step="hour", stepmode="backward"),
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
            range=[min_dt, max_dt],  # Initial range: last 24 hours
        ),
        legend=dict(orientation="h", yanchor="bottom", y=1.02, xanchor="right", x=1),
        margin=dict(l=40, r=40, t=40, b=40),
        hovermode="x unified",
    )

    return fig


# Run the app
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8050, debug=True)