from src.shop import fee as price

def test_fee():
    assert price(10) == 30
