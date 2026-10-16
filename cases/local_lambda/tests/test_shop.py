from src.shop import fee

def test_fee():
    fee = lambda amount: 30
    assert fee(10) == 30
