from src.cart import capped_total, clamped_double


def test_capped_total_stops_at_the_cap():
    assert capped_total([80, 50]) == 100
    assert capped_total([1, 2]) == 3


def test_clamped_double_doubles():
    assert clamped_double(2) == 4
