from src.shop import fee


def test_real_owner():
    assert fee(10) == 30


def test_helper_shadow_beside_real_call():
    def helper():
        fee = lambda a: 30
        return fee(10)
    helper()
    assert fee(10) == 30
