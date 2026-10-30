import numpy as np

from src.extra import signed


def test_signed():
    np.testing.assert_array_equal(signed("1"), -0x2)
