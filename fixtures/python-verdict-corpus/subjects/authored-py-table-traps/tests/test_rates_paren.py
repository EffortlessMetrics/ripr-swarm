import tablekit.rates_store  # noqa: F401
from tablekit.rates_paren import price


def test_silver_price_after_the_store_runs():
    assert price("silver") == 5
