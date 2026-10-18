from unittest.mock import Mock as M
from src.shop import fee


def test_local_aliased_mock():
    fee = M(return_value=30)
    assert fee(10) == 30
