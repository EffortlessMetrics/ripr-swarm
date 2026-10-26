from trapkit.core import reraised_assert


def test_reraised_assert():
    try:
        assert reraised_assert(3, 7) == 21
    except AssertionError:
        raise
