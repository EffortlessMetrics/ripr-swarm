import pytest

from grid_logical.core import can_enter, can_ship, is_visible, needs_review, should_retry


def test_ships_only_when_paid_and_in_stock():
    assert can_ship(True, True)
    assert not can_ship(True, False)
    assert not can_ship(False, True)


@pytest.mark.parametrize("flagged,large,want", [(False, False, False), (True, False, True), (False, True, True), (True, True, True)])
def test_review_table(flagged, large, want):
    assert needs_review(flagged, large) == want


def test_no_ticket_never_enters():
    for banned in (True, False):
        assert not can_enter(False, banned)


def check_visible(hidden, want):
    assert is_visible(hidden) == want, hidden


def test_visibility():
    check_visible(True, False)
    check_visible(False, True)


def test_retry_smoke():
    should_retry(True, False)
