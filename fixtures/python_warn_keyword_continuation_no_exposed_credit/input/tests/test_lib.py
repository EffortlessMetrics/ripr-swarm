from src.lib import headers


def test_headers():
    assert headers("x") == {"a": "x"}
