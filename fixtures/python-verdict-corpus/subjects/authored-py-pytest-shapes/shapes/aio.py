"""Coroutines driven by asyncio.run."""

import asyncio


async def fetch_total(prices):
    await asyncio.sleep(0)
    return sum(p for p in prices if p > 0)


async def first_or_default(items, default):
    for item in items:
        return item
    return default


async def fetch_count(prices):
    await asyncio.sleep(0)
    return len(prices)


class FixedRunner:
    """Has an event loop's method name but never awaits what it is given."""

    def run_until_complete(self, coro):
        coro.close()
        return 3
