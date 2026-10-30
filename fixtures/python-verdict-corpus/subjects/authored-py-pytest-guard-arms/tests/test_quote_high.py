from guards.quote_high import quote_high


def test_large_total_quote():
    assert quote_high(2000.0) == 2040.0
