import pytest

from discount import bulk_discount


@pytest.mark.parametrize("case", [(50, 0.0), (500, 0.1)])
def test_bulk_discount(case):
    quantity, rate = case
    assert bulk_discount(quantity) == rate
