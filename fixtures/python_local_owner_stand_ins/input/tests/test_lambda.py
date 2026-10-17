from src.shop import fee


def test_local_lambda():
    fee = lambda a: 30
    assert fee(10) == 30
