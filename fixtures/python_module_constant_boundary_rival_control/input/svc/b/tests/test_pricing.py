import pricing
from svc.a.src.pricing import discounted_total


def test_below():
    assert discounted_total(5_000) == 5_000


def test_above():
    assert discounted_total(20_000) == 18_000


def test_at():
    assert discounted_total(pricing.DISCOUNT_THRESHOLD) == 9_000
