from unittest.mock import Mock

from src.subject import on_parse


def test_on_parse_hands_the_number_to_its_handler():
    m = on_parse('1', Mock())
    m.assert_called_once_with(1)
