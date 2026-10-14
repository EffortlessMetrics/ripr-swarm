from src.cart import capped_total, doubled, floored_total


def test_capped_total_stops_at_the_cap():
    assert capped_total([80, 50]) == 100


def test_floored_total_adds_the_items():
    assert floored_total([1, 2]) == 3


def test_doubled_doubles():
    assert doubled(2) == 4
