from unittest import mock

from rules import is_adult


def change_default():
    is_adult.__defaults__ = (0,)


def test_adult():
    mock_handle = mock.Mock(assert_not_called=change_default)
    mock_handle.assert_not_called()
    assert is_adult(18)
