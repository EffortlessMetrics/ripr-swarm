from src.lib import Clock, dehumanize


def test_dehumanize():
    assert dehumanize(Clock(1), hours=2).t == 3
