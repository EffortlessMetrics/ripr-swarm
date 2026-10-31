import pytest
from src.shop import area

@pytest.mark.parametrize("a,b,want", [(1.5, 2.0, (3.0, 1))])
def test_area(a, b, want):
    a = 9
    assert area(a, b) == want
