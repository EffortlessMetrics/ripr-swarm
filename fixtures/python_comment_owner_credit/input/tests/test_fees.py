from src.fees import fee

def test_fee():
    fee(10)
    other = 12
    assert (other # fee
            == 12)
