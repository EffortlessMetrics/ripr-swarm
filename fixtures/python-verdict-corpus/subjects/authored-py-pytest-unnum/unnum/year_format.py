from functools import lru_cache


@lru_cache
def format_year(year):
    return "%03d" % (year % 100)
