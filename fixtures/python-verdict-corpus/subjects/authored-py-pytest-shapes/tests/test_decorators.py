from shapes import decorators


def test_flaky_succeeds_within_three_attempts():
    decorators.reset()
    assert decorators.flaky() == "ok"


def test_double_returns_twice_the_argument():
    decorators.reset()
    assert decorators.double(4) == 8
