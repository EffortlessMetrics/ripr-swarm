import pytest

from trapkit.core import filled_table


@pytest.mark.parametrize("a,b,want", [(3, 7, 21)])
def test_filled_table(a, b, want):
    assert filled_table(a, b) == want
