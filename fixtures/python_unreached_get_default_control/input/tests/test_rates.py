from src.rates import lookup


def test_lookup_unknown_tier():
    assert lookup("platinum") == 1
