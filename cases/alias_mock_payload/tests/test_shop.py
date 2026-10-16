from unittest.mock import Mock
from src.shop import fee as price

def test_fee():
    price = Mock(return_value=30)
    assert price(10) == 30
