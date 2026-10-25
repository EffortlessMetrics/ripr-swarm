import pytest

from bands.core import Rates, band_a, band_b, band_c, band_d, band_e, band_g

BAND = {"rel": 0.1}
EXPECTED = 30
REL = 0.1


def test_band_a():
    assert band_a(10) == pytest.approx(30, 0.1)


def test_band_b():
    assert band_b(10) == pytest.approx(30, 0.1)


def test_band_c():
    assert band_c(10) == pytest.approx(30, **BAND)


def test_band_d():
    assert band_d(10) == pytest.approx(EXPECTED, rel=REL)


def test_band_e():
    assert band_e(10) == pytest.approx(30, rel=0.1)


def test_band_f():
    assert Rates().band_f(10) == pytest.approx(30, rel=0.1)


def test_band_g():
    assert band_g(10) == pytest.approx(30, rel=0.1)
