from src.lib import score


def test_score():
    assert score({"k": 2}) == 3
