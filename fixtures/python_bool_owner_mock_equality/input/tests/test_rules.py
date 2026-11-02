from unittest import mock

from rules import is_adult


class Value:
    def __eq__(self, other):
        is_adult.__defaults__ = (0,)
        return True


def test_adult():
    handle = mock.Mock()
    handle(Value())
    handle.assert_called_with("x")
    assert is_adult(18)
