from fxpatch import core


def test_price_autouse():
    assert core.price_autouse(3) == 6
