import pytest

from shop import handling_fee


@pytest.mark.parametrize("amount, fee", [(99, 5)])
def test_handling_fee(amount, fee):
    assert handling_fee(amount) == fee
