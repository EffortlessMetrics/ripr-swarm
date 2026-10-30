from rates.table import price


def test_silver_price():
    assert price("silver") == 5
