from rates.core import rate_a, rate_b, rate_c, rate_d, rate_e

OTHER = 5


def test_rate_a():
    assert rate_a(10) == 30 and rate_a(10) > 0


def test_rate_b():
    assert rate_b(10) > 0 and OTHER == 5


def test_rate_c():
    assert rate_c(10) > 0 and rate_c(10) < 1000


def test_rate_d():
    assert rate_d(10) == 30 or rate_d(10) == 31


def test_rate_e():
    assert not rate_e(10) == 31
