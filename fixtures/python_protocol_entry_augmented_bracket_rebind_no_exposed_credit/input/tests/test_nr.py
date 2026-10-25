from src.nr import numeric_range


def test_reversed_after_rebinding_the_receiver():
    r = numeric_range(0)
    (r) += [1]
    assert list(reversed(r)) == [1]
