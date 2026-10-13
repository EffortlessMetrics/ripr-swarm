import pytest

from src.subject import perr_exc_length, perr_exc_not_none


def test_raises_as_exc_not_none():
    with pytest.raises(KeyError) as exc:
        perr_exc_not_none("")
    assert exc.value is not None


def test_raises_as_exc_length():
    with pytest.raises(KeyError) as exc:
        perr_exc_length("")
    assert len(str(exc.value)) == 7
