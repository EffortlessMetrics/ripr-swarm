from src.lib import pick


def test_pick():
    assert pick(None, 3, True) == 3
    assert pick(5, 3, False) == 5
