from src.shop import fee as real_fee
from src.fixed import fixed as quote


def test_fee_return():
    real_fee(10)
    assert quote(10) == 30
