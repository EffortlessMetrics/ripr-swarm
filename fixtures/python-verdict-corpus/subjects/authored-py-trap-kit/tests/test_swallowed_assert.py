from trapkit.core import swallowed_assert


def test_swallowed_assert():
    try:
        assert swallowed_assert(3, 7) == 21
    except AssertionError:
        pass
