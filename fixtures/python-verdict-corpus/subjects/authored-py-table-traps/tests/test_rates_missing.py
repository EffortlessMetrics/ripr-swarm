from tablekit.rates_missing import price


def test_missing_key_reads_the_fallback():
    assert price(3) == 5
