from trapkit.core import tuple_asserted


def test_tuple_asserted():
    assert (tuple_asserted(3, 7) == 21, "three sevens")  # noqa: F631
