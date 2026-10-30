import pytest

from trapkit.core import empty_table


@pytest.mark.parametrize("a,b,want", [])
def test_empty_table(a, b, want):
    assert empty_table(a, b) == want
