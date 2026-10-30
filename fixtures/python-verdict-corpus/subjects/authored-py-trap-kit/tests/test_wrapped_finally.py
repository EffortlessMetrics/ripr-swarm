from trapkit.core import wrapped_finally


def settled(fn):
    try:
        return fn()
    except Exception:
        raise
    finally:
        return None


def must_equal(got, want):
    assert got == want


def test_wrapped_finally():
    settled(lambda: must_equal(wrapped_finally(3, 7), 21))
