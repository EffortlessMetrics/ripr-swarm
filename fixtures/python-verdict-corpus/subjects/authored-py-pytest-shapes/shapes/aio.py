"""Coroutines driven by asyncio.run."""

import asyncio


async def fetch_total(prices):
    await asyncio.sleep(0)
    return sum(p for p in prices if p > 0)


async def first_or_default(items, default):
    for item in items:
        return item
    return default
