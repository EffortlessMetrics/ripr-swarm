import numpy as np
import pytest

from src.extra import widen


def test_widen_differs():
    with pytest.raises(AssertionError):
        np.testing.assert_array_equal(widen("1"), [99])
