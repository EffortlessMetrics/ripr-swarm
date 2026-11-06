from fxpatch import core


def test_price_env():
    assert core.price_env(3) == 6
