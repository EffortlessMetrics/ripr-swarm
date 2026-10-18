from guards.quote_rebound import quote_rebound


def test_rebound_quote():
    assert quote_rebound(100.0) == 2040.0
