from src.lib import pick


def test_pick():
    assert pick(None, 3) == 3
    assert list(map(pick, [0], [3])) == [3]
