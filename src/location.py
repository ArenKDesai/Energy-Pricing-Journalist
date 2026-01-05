import requests
import json
from pyproj import Transformer
import polars as pl


def __get_locations(hub: bool) -> pl.DataFrame:
    """
    Docstring for __get_locations

    :return: Location data for nodes in MISO LMP contour map.
    :rtype: DataFrame
    """
    # Request
    url = "https://api.misoenergy.org/MISORTWDDataBroker/DataBrokerServices.asmx"
    params = {
        "messageType": "getvectorsource",
        "nodeTypes": "HUB" if hub else "GEN,INT,LZN",
    }
    headers = {
        "Accept": "application/xml",
    }

    # Response
    try:
        response = requests.get(url, params=params, headers=headers, timeout=10)
        response.raise_for_status()

    except requests.exceptions.RequestException as e:
        print(f"Request failed: {e}")

    projected_data = json.loads(response.text)

    # String from MISO's request header
    miso_proj = "+proj=lcc +lat_1=33 +lat_2=45 +lat_0=0 +lon_0=-100 +x_0=0 +y_0=0 +ellps=WGS84 +units=m +no_defs"
    transformer = Transformer.from_crs(miso_proj, "epsg:4326", always_xy=True)

    # Project to lat/lon
    for feature in projected_data["f"]:
        x = feature["g"]["c"][0]
        y = feature["g"]["c"][1]

        lat, lon = transformer.transform(x, y)
        feature["p"].append({"lat": lat, "lon": lon})

    # Organize data into polars df
    locations = []
    node_types = []
    regions = []
    lats = []
    lons = []
    for node in projected_data["f"]:
        for i, feature in enumerate(node["p"]):
            match i:
                case 0:
                    locations.append(feature)
                case 1:
                    node_types.append(feature)
                case 2:
                    regions.append(feature)
                case 3:
                    lats.append(feature["lat"])
                    lons.append(feature["lon"])

    df = pl.DataFrame(
        {
            "location": locations,
            "node_type": node_types,
            "region": regions,
            "latitude": lats,
            "longitude": lons,
        }
    )
    return df


def get_location_df() -> pl.DataFrame:
    nodes = __get_locations(hub=False)
    hubs = __get_locations(hub=True)
    locations = pl.concat([nodes, hubs]).sort(by="location")
    return locations