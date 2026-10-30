import numpy as np

from src.extra import mirror


def test_mirror():
    np.testing.assert_array_equal(mirror("1"), mirror("1"))
