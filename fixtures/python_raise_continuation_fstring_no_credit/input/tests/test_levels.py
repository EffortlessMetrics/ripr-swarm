import pytest
from src.levels import parse


@pytest.mark.parametrize("text, expected", [("low", "LOW"), ("hi", "HI")])
def test_parse(text, expected):
    assert parse(text, 5) == expected


def test_too_long():
    with pytest.raises(ValueError):
        parse("verbose", 3)
