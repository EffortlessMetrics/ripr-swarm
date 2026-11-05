from wrapper import greet


def test_greet_mentions_name():
    assert greet('bob') == 'hi, bob'
