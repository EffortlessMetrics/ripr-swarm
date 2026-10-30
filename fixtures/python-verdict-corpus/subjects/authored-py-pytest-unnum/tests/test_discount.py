from unnum.discount import apply_discount


def test_discount_applies_above_threshold():
    assert apply_discount(100, 50) == 90


def test_no_discount_below_threshold():
    assert apply_discount(10, 50) == 10
