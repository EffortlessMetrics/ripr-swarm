import pytest

from grid_arithmetic.core import area, line_total, scaled, seconds, tip


def test_three_at_seven_costs_twenty_one():
    assert line_total(3, 7) == 21


@pytest.mark.parametrize("width,height,want", [(2, 3, 6), (4, 5, 20), (1, 9, 9)])
def test_area_table(width, height, want):
    assert area(width, height) == want


def test_scaling_by_one_keeps_the_quantity():
    for qty in range(1, 5):
        assert scaled(qty, 1) == qty


def check_tip(bill, rate, want):
    assert tip(bill, rate) == want


def test_tip():
    check_tip(40, 2, 80)


def test_seconds_smoke():
    seconds(3)
