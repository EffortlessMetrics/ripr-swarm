import pytest

from grid_constant.core import commission, handling, late_fee, rounding_unit, sales_tax


def test_tax_on_a_thousand():
    assert sales_tax(1000) == 80


@pytest.mark.parametrize("days,want", [(0, 0), (1, 5), (4, 20)])
def test_late_fee_table(days, want):
    assert late_fee(days) == want


def test_commission_stays_within_the_sale():
    for sale in range(0, 1000, 100):
        assert 0 <= commission(sale) <= sale


def check_handling(weight, want):
    assert handling(weight) == want


def test_handling():
    check_handling(2, 24)


def test_rounding_smoke():
    rounding_unit(37)
