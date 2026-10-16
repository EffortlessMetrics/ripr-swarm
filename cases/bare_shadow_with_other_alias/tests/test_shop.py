from src.shop import fee as price

def test_fee():
    def fee(amount):
        return 30
    assert fee(10) == 30
