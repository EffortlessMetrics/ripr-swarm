import pytest
from src.levels import other, parse


def test_blank():
    with pytest.raises(ValueError) as excinfo:
        other("")
    assert str(excinfo.value) == parse("blank level").lower()
