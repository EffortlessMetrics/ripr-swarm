import pytest

from grid_boundary.core import can_retire, free_shipping, is_adult, overweight, passes


def test_eighteen_is_adult_and_seventeen_is_not():
    assert is_adult(18)
    assert not is_adult(17)


@pytest.mark.parametrize("cents,want", [(0, False), (4999, False), (5000, True), (12000, True)])
def test_free_shipping_table(cents, want):
    assert free_shipping(cents) == want


def test_retirement_is_monotonic_in_age():
    for age in range(0, 120):
        if can_retire(age):
            assert can_retire(age + 1), age
    assert not can_retire(0)
    assert can_retire(119)


def check_pass(score, want):
    assert passes(score) == want, score


def test_pass_mark_is_fifty():
    check_pass(50, True)
    check_pass(49, False)


def test_overweight_smoke():
    overweight(2500)
