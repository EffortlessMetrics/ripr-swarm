from numpy.testing import assert_equal

from src.extra import pair


def test_pair():
    assert_equal(pair("1"), (2, 3))
