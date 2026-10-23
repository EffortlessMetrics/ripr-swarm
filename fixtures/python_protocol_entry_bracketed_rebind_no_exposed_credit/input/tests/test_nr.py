from src.nr import numeric_range


def test_reversed_after_rebinding_the_receiver():
    r = numeric_range(0)
    ((r), x) = [1], 2
    assert list(reversed(r)) == [1]


def test_reversed_after_nonadjacent_tuple_rebind():
    r = numeric_range(0)
    (x, (r)) = 2, [1]
    assert list(reversed(r)) == [1]


def test_reversed_after_nonadjacent_list_rebind():
    r = numeric_range(0)
    [a, [r]] = [2, [[1]]]
    assert list(reversed(r)) == [1]


def test_reversed_after_unbracketed_outer_rebind():
    r = numeric_range(0)
    x, (y, (r)) = 1, (2, [1])
    assert list(reversed(r)) == [1]


def test_reversed_after_chained_outer_rebind():
    r = numeric_range(0)
    x = (y, (r)) = 2, [1]
    assert list(reversed(r)) == [1]


def test_reversed_after_inline_loop_rebind():
    r = numeric_range(0)
    for (x, (r)) in [(2, [1])]: pass
    assert list(reversed(r)) == [1]


def test_reversed_after_inline_with_rebind():
    r = numeric_range(0)
    with manager() as (x, (r)): pass
    assert list(reversed(r)) == [1]
