"""Decorator factories and wrappers."""

import functools

CALL_LOG = []
_attempts = {"n": 0}


def retry(times):
    def wrap(fn):
        @functools.wraps(fn)
        def inner(*args, **kwargs):
            last = None
            for _ in range(times):
                try:
                    return fn(*args, **kwargs)
                except ValueError as exc:
                    last = exc
            raise last

        return inner

    return wrap


def logged(fn):
    @functools.wraps(fn)
    def inner(*args):
        CALL_LOG.append(fn.__name__)
        return fn(*args)

    return inner


def reset():
    _attempts["n"] = 0
    CALL_LOG.clear()


@retry(3)
def flaky():
    _attempts["n"] += 1
    if _attempts["n"] < 3:
        raise ValueError("not yet")
    return "ok"


@logged
def double(x):
    return x * 2
