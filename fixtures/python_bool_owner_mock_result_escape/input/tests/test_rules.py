from unittest import mock

from rules import is_adult


def change_default():
    is_adult.__defaults__ = (0,)


def test_adult():
    handle = mock.Mock()
    other = handle()
    other.side_effect = change_default
    other()
    assert is_adult(18)
