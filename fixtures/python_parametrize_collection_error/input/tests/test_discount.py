import pytest

from discount import bulk_discount


@pytest.mark.parametrize("q", [1], indirect=None)
@pytest.mark.parametrize("quantity, rate", [(100, 0.1), (50, 0.0)])
def test_bulk_discount(q, quantity, rate):
    assert bulk_discount(quantity) == rate
