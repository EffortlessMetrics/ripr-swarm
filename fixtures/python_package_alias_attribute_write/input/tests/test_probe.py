from . import tomllib


def test_loads():
    assert tomllib.loads("") == {}
