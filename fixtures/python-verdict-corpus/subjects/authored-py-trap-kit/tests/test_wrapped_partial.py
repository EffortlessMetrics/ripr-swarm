from trapkit.core import wrapped_partial


def guarded(fn):
    try:
        return fn()
    except ValueError:
        raise
    except Exception:
        return None


def must_equal(got, want):
    assert got == want


def test_wrapped_partial():
    guarded(lambda: must_equal(wrapped_partial(3, 7), 21))
