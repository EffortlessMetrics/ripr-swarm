import pytest

from unnum.negative import reject_negative


def test_rejects_negative_amount():
    with pytest.raises(ValueError):
        reject_negative(-1)
