from src.rates import lookup


def test_lookup_silver():
    assert lookup("silver") == 5
