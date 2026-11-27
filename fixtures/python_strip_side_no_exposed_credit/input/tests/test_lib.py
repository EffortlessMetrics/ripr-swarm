from src.lib import clean


def test_clean():
    assert clean("  a") == "a"
