from src.lib import ok


def test_ok():
    assert ok(1, 2) is True
    assert ok(-1, -2) is False
