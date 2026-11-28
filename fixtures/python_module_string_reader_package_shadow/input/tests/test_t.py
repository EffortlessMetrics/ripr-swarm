from fx.core import greet


def test_greet_mentions_name():
    assert 'bob' in greet('bob')
