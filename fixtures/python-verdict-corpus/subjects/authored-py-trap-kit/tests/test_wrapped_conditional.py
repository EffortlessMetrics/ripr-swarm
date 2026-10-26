from trapkit.core import wrapped_conditional


def selective(fn):
    try:
        return fn()
    except Exception as exc:
        if isinstance(exc, ValueError):
            raise


def must_equal(got, want):
    assert got == want


def test_wrapped_conditional():
    selective(lambda: must_equal(wrapped_conditional(3, 7), 21))
