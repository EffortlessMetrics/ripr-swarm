import importlib
from shop import pricing
from shop.pricing import DISCOUNT_THRESHOLD, discounted_total


def test_discount(monkeypatch):
    mp = monkeypatch
    pricing.__setattr__("DISCOUNT_THRESHOLD", 5)
    assert discounted_total(20_000) == 18_000
    assert discounted_total(DISCOUNT_THRESHOLD) == 9_000
