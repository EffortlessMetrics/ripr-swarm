import pytest
from src.shop import area, float_area, boxed_area, hashed_area


@pytest.mark.parametrize("a,b,want", [(3, 7, 21)])
def test_area(a, b, want):
    assert area(a, b) == want


@pytest.mark.parametrize("a,b,want", [(1.5, 2.0, 3.0), (-4.0, 2.0, -8.0)])
def test_float_area(a, b, want):
    assert float_area(a, b) == want


@pytest.mark.parametrize("a,b,want", [(3, 7, (21,)), (4, 8, (32,))])
def test_boxed_area(a, b, want):
    assert boxed_area(a, b) == want


@pytest.mark.parametrize("a,b,want", [(3, 7, (21, '#'))])
def test_hashed_area(a, b, want):
    assert hashed_area(a, b) == want
