from src.lib import money


def test_money():
    assert money(1.05) == 1.1
