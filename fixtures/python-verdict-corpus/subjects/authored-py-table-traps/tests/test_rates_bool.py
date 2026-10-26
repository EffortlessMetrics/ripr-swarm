from tablekit.rates_bool import price


def test_bool_key_selects_the_integer_entry():
    assert price(True) == 5
