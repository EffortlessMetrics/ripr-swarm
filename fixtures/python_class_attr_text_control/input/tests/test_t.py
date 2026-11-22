import pytest

from fx.core import Length, ValidationError


def test_within_limit():
    assert Length(max=3)("abc") == "abc"


def test_too_long():
    with pytest.raises(ValidationError) as exc:
        Length(max=1)("ab")
    assert (
        len(exc.value.args) == 1
        and str(exc.value) == "Longer than maximum length 1."
    )
