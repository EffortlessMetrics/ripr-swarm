from unittest import mock

from rules import is_adult


def test_adult(monkeypatch):
    monkeypatch.setenv("HOME", "/tmp/home")
    assert is_adult(18)
    assert not is_adult(17)


def test_handle():
    handle = mock.MagicMock()
    handle.assert_not_called()
