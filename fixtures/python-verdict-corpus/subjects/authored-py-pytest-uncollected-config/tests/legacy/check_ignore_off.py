from unconfig.core import ignore_off


def test_ignore_off():
    assert ignore_off(3, 7) == 21
