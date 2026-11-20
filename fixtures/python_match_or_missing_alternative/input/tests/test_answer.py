import pytest

from src.answer import to_bool


@pytest.mark.parametrize("val, expected", [("yes", True), ("on", False)])
def test_to_bool(val, expected):
    assert to_bool(val) == expected
