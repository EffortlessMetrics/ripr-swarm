import pytest

from idioms.validate import parse_age


def test_parse_age_accepts_a_positive_number():
    assert parse_age("5") == 5


def test_parse_age_rejects_negative_numbers():
    with pytest.raises(ValueError, match="non-negative"):
        parse_age("-1")
