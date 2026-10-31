from src.pricing import tier


def test_gold():
    assert tier(80) == "gold"


def test_edge():
    s = 50
    assert tier(s) == "basic"
