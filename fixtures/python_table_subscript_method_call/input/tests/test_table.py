import rates.touch  # noqa: F401
from rates.table import price


def test_silver_price_after_import():
    assert price("silver") == 5
