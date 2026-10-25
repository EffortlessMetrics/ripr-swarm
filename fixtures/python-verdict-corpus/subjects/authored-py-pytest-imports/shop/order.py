from shop import rates


def checkout(cents):
    label = "order"
    return cents + rates.tax(cents)


def rush(cents):
    tag = "rush"
    return cents + rates.fee(cents)
