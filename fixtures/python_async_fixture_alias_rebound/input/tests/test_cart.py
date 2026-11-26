import pytest
from pytest_asyncio import fixture as fx

from src.cart import Cart

if True:
    from pytest import fixture as fx


@fx
async def cart():
    return Cart(200)


@pytest.mark.asyncio
async def test_discounted(cart):
    assert await cart.discounted() == 190
