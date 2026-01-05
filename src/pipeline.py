import requests
import polars as pl


def __get_prices(hub: bool):
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
    try:
        response = requests.post(url, headers=headers, data=payload, timeout=10)

        response.raise_for_status()

    except requests.exceptions.RequestException as e:
        print(f"An error occurred: {e}")
        # TODO: return empty df

    df = pl.DataFrame(response.json()["data"])
    if "NSI" in df.columns:
        df = df.drop("NSI")
    df = df.rename({col: col.lower() for col in df.columns})

    value_cols = ["lmp", "mcc", "mlc"]
    df = df.select(["location"] + value_cols)
    for col in value_cols:
        df = df.with_columns(pl.col(col).cast(pl.Float32))
    df = df.with_columns(pl.lit(response.status_code).cast(pl.Int16).alias("status_code"))

    return df


def reload_prices_df(metadata: bool = True):
    nodes = __get_prices(hub=False)
    hubs = __get_prices(hub=True)
    prices = pl.concat([nodes, hubs]).sort(by="location")

    return prices