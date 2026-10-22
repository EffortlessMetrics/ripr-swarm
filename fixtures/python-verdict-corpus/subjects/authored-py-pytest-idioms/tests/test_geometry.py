import pytest

from idioms.geometry import circle_area


def test_circle_area_is_close_to_pi_r_squared():
    assert circle_area(2) == pytest.approx(12.566, rel=1e-3)
