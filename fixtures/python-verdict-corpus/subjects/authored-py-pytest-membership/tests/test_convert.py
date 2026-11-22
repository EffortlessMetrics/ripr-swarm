import pytest

from truthy.convert import to_bool


@pytest.mark.parametrize("val", ["true", "t", "yes", "on", "1"])
def test_truthy(val):
    assert to_bool(val) == True


@pytest.mark.parametrize("val", ["false", "f", "no", "n", "off", "0"])
def test_falsy(val):
    assert to_bool(val) == False
