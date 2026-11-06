from unittest import mock

from rules import is_adult


def test_adult():
    assert is_adult(18)


def test_adult_when_stubbed():
    with mock.patch("rules.is_adult", return_value=False) as stub:
        stub(18)
        stub.assert_called_once_with(18)
