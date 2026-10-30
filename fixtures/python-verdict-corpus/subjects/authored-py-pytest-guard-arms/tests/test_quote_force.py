from guards.quote_force import quote_force


def test_forced_quote():
    assert quote_force(100.0, True) == 102.0
