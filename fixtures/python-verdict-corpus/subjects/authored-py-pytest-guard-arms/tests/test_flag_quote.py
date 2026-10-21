from guards.flag_quote import quote_flag


def test_flag_quote():
    assert quote_flag(True) == 20
