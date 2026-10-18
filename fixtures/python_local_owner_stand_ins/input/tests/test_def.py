from src.shop import fee


def test_local_def():
    def fee(a):
        return 30
    assert fee(10) == 30
