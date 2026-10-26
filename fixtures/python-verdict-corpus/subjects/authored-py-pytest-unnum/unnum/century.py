from functools import lru_cache


@lru_cache(maxsize=None)
def century_index(year):
    return year % 1000
