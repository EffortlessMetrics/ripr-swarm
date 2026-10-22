import asyncio

from shapes.aio import fetch_total, first_or_default


def test_fetch_total_skips_negative_prices():
    assert asyncio.run(fetch_total([3, -1, 4])) == 7


def test_first_or_default_returns_the_default_for_empty_input():
    assert asyncio.run(first_or_default([], "x")) == "x"
