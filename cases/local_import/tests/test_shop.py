def test_fee():
    from src.shop import fee
    assert fee(10) == 30
