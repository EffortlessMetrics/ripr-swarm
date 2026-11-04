import pytest

from src.app import scale


def test_scale():
    with pytest.raises(ValueError, match="neg"):
        scale(-1)
    assert scale(2) > 0
