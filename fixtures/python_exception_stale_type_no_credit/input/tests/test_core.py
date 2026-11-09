import pytest
from fx.core import parse

def test_parse():
    with pytest.raises(ValueError, match="m"):
        parse('abcdefghijklmnop')
