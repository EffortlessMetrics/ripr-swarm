from shapes.money import Money


def test_add_sums_cents():
    assert Money(5) + Money(7) == Money(12)


def test_ordering():
    assert Money(1) < Money(2)
    assert not Money(2) < Money(1)


def test_not_equal_to_a_plain_number():
    assert Money(1) != 1


def test_repr_names_the_cents():
    assert repr(Money(3)) == "Money(3)"


def test_truthiness():
    assert Money(1)
    assert not Money(0)
