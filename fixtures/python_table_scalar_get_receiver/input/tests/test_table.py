from rates.table import price


def test_price_of_a_scalar_tier():
    assert price(5) == 5
