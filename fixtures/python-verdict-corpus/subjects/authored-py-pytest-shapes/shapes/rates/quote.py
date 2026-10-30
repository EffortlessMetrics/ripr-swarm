from .table import lookup


def quote(tier, units):
    return lookup(tier) * units
