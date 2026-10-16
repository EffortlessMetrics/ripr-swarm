from src.shop import fee

def test_fee():
    def helper(amount):
        return 30
    assert fee(10) == 30
