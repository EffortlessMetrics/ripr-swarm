from src.clock import Clock


def test_days():
    assert Clock(48).days() == 2
