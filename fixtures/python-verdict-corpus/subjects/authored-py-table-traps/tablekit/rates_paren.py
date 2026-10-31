PAREN_RATES = ({"gold": 10, "silver": 5})


def price(tier):
    return PAREN_RATES.get(tier, 0)
