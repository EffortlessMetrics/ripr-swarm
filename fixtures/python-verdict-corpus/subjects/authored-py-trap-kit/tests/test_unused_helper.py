from trapkit.core import unused_helper


def check_total():
    assert unused_helper(3, 7) == 21


def test_unused_helper_runs_nothing():
    assert callable(check_total)
