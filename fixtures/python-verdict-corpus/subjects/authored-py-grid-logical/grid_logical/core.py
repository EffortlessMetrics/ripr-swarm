"""Boolean logic, one function per test style."""


def can_ship(paid, in_stock):
    return paid and in_stock


def needs_review(flagged, large):
    return flagged or large


def can_enter(has_ticket, banned):
    return has_ticket and not banned


def is_visible(hidden):
    return not hidden


def should_retry(timed_out, attempts_left):
    return timed_out and attempts_left
