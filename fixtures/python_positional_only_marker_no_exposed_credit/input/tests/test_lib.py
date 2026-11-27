from src.lib import key


def test_key_returns_args():
    assert key(None, 1, 2) == (1, 2)
