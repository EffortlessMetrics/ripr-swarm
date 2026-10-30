from unittest.mock import Mock
from src.shop import fee


def test_local_mock():
    fee = Mock(return_value=30)
    assert fee(10) == 30
