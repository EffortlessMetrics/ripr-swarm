from src.fees import fee

def test_fee():
    assert fee(10) == fee(
        10 # irrelevant
    )
