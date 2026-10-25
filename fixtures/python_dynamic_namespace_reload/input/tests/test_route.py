import importlib
from types import SimpleNamespace
from shop import route
from shop.route import checkout, response


def test_checkout(monkeypatch):
    mp = monkeypatch
    importlib.reload(route)
    checkout()
    assert response.status_code == 422
