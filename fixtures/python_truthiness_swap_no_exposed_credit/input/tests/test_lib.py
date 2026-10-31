from src.lib import pick


def test_pick():
    assert pick(None, 3) == 3
    assert pick(5, 3) == 5
