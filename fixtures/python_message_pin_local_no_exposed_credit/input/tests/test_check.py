import pytest

from src.check import check


def test_negative_is_rejected():
    with pytest.raises(ValueError, match="negative value"):
        check(-1)
