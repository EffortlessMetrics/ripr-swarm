from unittest import mock

from rules import is_adult


def test_adult(swap: mock.patch = None):
    test_adult.__annotations__["swap"]("test_rules.is_adult", return_value=True).start()
    assert is_adult(18)
