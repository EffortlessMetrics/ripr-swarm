import pytest

from unrun.core import skip_tuple


@pytest.mark.parametrize("a,b,want", ())
def test_skip_tuple(a, b, want):
    assert skip_tuple(a, b) == want
