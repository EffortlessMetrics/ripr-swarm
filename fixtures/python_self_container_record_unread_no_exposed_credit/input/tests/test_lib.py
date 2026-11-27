from src.lib import Clock


def test_tick():
    assert Clock(1).tick(2) == 3
