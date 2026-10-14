import numpy as np

from src.arrays import parse


def test_parse():
    np.testing.assert_array_equal(parse("1"), [2])
