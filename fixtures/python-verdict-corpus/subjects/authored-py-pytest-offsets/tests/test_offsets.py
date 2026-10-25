import pytest

from offsets.core import tier_a, tier_b, tier_c, tier_d, tier_e


def test_tier_a():
    assert tier_a(4) == "low"
    assert tier_a(5) == "high"


def test_tier_b():
    assert tier_b(6) == "high"
    assert tier_b(5) == "low"


def test_tier_c():
    assert tier_c(0) == "low"
    assert tier_c(100) == "high"


@pytest.mark.parametrize("n, threshold, expected", [(4, 5, "low"), (5, 5, "high")])
def test_tier_d(n, threshold, expected):
    assert tier_d(n, threshold) == expected


@pytest.mark.parametrize("n, threshold, expected", [(0, 5, "low"), (9, 5, "high")])
def test_tier_e(n, threshold, expected):
    assert tier_e(n, threshold) == expected
