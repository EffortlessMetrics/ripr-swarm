import pytest

from fmt.stamp import seconds


def test_seconds():
    assert seconds(1000) == 1000.0


def test_negative_seconds():
    with pytest.raises(ValueError, match="not a positive timestamp"):
        seconds(-10)
