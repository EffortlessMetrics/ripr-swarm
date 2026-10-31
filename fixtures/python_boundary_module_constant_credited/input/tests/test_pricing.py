from src.pricing import tier

THRESHOLD = 50


def test_gold():
    assert tier(80) == "gold"


def test_edge():
    assert tier(THRESHOLD) == "basic"
