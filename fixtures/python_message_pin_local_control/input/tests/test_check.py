import pytest

from src.check import check


def test_negative_is_rejected():
    with pytest.raises(ValueError, match="at most zero"):
        check(-1)
