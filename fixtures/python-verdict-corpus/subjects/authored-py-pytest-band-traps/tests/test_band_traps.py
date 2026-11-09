import pytest

from band_traps.cached import band
from band_traps.redirect import Rates


def test_method_band():
    assert Rates().band(10) == pytest.approx(30, rel=0.1)


def test_cached_band():
    assert band(10) == pytest.approx(30, rel=0.1)
