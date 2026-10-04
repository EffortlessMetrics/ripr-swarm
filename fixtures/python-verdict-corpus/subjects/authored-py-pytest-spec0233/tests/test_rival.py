from src.other import parse_rival


def test_rival_parse():
    assert parse_rival("1") == 2
