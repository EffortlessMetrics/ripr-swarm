from shop import rates


def checkout(cents):
    return cents + rates.tax(cents)


def rush(cents):
    return cents + rates.fee(cents)
