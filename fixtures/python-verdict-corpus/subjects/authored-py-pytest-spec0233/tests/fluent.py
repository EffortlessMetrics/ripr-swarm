"""A minimal fluent assertion helper, written for this corpus."""


class _Subject:
    def __init__(self, actual):
        self.actual = actual

    def is_equal_to(self, expected):
        if self.actual != expected:
            raise AssertionError(f"{self.actual!r} != {expected!r}")
        return self


def assert_that(actual):
    return _Subject(actual)
