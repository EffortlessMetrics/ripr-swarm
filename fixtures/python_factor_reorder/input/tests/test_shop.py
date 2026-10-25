import pytest
from src.shop import area


@pytest.mark.parametrize("a,b,want", [(3, 7, 21)])
def test_area(a, b, want):
    assert area(a, b) == want
