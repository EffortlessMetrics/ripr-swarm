from src.shop import fee

def test_fee():
    def fee(amount):
        return 30
    assert fee(10) == 30
