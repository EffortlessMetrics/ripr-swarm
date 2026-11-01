import pytest

from fxpatch import core


@pytest.fixture(autouse=True)
def _stub_price(monkeypatch):
    monkeypatch.setattr(core, "price_autouse", lambda x: x * 2)
