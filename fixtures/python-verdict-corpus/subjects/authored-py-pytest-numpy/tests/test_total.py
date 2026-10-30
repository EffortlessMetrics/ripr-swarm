from assertpy import assert_that

from src.extra import total


def test_total():
    assert_that(total("1")).is_equal_to(2)
