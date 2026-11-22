from fx.core import over


def test_over():
    assert over(10, max_age=10) == 0
    assert over(11, max_age=10) == 1
