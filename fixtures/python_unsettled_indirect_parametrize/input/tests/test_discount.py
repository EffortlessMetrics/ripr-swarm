import pytest

from discount import bulk_discount

IND = True


@pytest.mark.parametrize("quantity", [100], indirect=IND)
def test_bulk_discount_at_threshold(quantity):
    assert bulk_discount(quantity) == 0.1
