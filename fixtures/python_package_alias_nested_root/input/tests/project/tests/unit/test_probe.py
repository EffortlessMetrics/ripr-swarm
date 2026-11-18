from . import toml


def test_loads():
    assert toml.loads("") == {}
