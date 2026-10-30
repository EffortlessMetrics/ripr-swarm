import numpy as np

from src.extra import scaled


def test_scaled():
    np.testing.assert_allclose(scaled(2.0), 4.0)
