from trapkit.core import wrapped_through


def logged(fn):
    try:
        return fn()
    except Exception:
        raise


def must_equal(got, want):
    assert got == want


def test_wrapped_through():
    logged(lambda: must_equal(wrapped_through(3, 7), 21))
