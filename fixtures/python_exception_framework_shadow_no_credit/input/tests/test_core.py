import pytest
from fx.core import parse, TooLongError

def test_parse():
    with pytest.raises(TooLongError):
        parse('abcdefghijklmnop')
