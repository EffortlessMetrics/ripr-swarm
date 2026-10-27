import rates.store  # noqa: F401
from rates.table import price


def test_silver_price_after_the_store_runs():
    assert price("silver") == 5
