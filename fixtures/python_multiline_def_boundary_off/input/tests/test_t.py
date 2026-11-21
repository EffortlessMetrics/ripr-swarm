from fx.core import expired


def test_expired():
    assert expired(105, max_age=10) == 0
    assert expired(111, max_age=10) == 1
