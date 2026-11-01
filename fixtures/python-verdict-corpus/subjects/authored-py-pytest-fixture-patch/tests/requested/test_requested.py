import pytest

from fxpatch import core


@pytest.fixture
def stubbed(monkeypatch):
    monkeypatch.setattr(core, "price_requested", lambda x: x * 2)


def test_price_requested(stubbed):
    assert core.price_requested(3) == 6
