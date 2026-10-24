from shapes.rates import lookup
from shapes.rates.quote import quote


def test_quote_gold():
    assert quote("gold", 3) == 30


def test_lookup_silver_through_the_package_export():
    assert lookup("silver") == 5
