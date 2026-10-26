from functools import lru_cache


def band_a(x):
    return x * 3


def band_b(x):
    return x * 3


def band_c(x):
    return x * 3


def band_d(x):
    return x * 3


def band_e(x):
    y = x * 3
    return y


class Rates:
    def band_f(self, x):
        return x * 3


@lru_cache(maxsize=None)
def band_g(x):
    return x * 3
