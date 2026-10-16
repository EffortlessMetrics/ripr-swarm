from src.shop import fee

def test_fee():
    def helper():
        fee = lambda amount: 30
        return fee(10)
    assert fee(10) == 30
