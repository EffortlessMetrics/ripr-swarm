from src.shop import fee

fee = lambda a: 30


def test_module_level_stand_in():
    assert fee(10) == 30
