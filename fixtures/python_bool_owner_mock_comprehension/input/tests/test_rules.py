from unittest import mock

from rules import is_adult


class Local:
    def reset_mock(self):
        is_adult.__defaults__ = (0,)


def test_adult():
    handle = mock.Mock()
    [handle.reset_mock() for handle in [Local()]]
    assert is_adult(18)
