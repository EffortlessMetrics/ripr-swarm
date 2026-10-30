from tablekit.rates_plain import price


def test_silver_price():
    assert price("silver") == 5
