from src.q import build_query


def test_build_query():
    assert "WHERE b = 1" in build_query()
