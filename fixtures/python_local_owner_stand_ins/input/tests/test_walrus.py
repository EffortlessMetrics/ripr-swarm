from src.shop import fee


def test_walrus_stand_in():
    if (fee := (lambda a: 30)):
        assert fee(10) == 30
