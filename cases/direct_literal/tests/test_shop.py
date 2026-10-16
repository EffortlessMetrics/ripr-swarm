from src.shop import fee

def test_fee():
    assert fee(10) == 30
