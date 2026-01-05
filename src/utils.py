import polars as pl


def records_equal(df1: pl.DataFrame, df2: pl.DataFrame) -> bool:
    """
    Test if two pricing records are equal. Drops columns that interfere with this.

    :param df1: First polars dataframe.
    :type df1: pl.DataFrame
    :param df2: Second polars dataframe.
    :type df2: pl.DataFrame
    :return: Whether the dataframes are equal.
    :rtype: bool
    """
    df1_comparable = df1.drop("datetime")
    df2_comparable = df2.drop("datetime")
    return df1_comparable.equals(df2_comparable)
