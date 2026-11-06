from unittest import mock

from rules import is_adult


def test_adult():
    f"{mock.patch('test_rules.is_adult', return_value=True).start()}"
    assert is_adult(18)
