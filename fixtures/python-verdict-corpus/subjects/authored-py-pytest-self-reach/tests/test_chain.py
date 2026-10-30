from reach.chain import Chain


def test_chain_value():
    assert Chain().run() == 6
