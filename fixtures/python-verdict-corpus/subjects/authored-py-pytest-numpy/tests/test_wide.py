import numpy as np

from src.extra import wide


def test_wide():
    np.testing.assert_array_equal(wide("1"), 0x100000000000000000000000000000001)
