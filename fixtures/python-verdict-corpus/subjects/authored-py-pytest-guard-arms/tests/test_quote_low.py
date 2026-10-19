from guards.quote_low import quote_low


def test_small_total_quote():
    assert quote_low(100.0) == 105.0
