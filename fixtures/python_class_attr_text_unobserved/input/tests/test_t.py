import pytest

from fx.core import Length, ValidationError


def test_within_limit():
    assert Length(max=3)("abc") == "abc"


def test_too_long():
    with pytest.raises(ValidationError, match="Longer than"):
        Length(max=1)("ab")
