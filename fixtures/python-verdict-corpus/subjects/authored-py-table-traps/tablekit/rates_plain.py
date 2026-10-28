PLAIN_RATES = ({"gold": 10, "silver": 5})


def price(tier):
    return PLAIN_RATES.get(tier, 0)
