from hostile.settings import price


def test_silver_price_after_the_store_runs():
    assert price("silver") == 5
