MISSING_RATES = {1: 5, 2: 5}


def price(tier):
    return MISSING_RATES.get(tier, MISSING_RATES[2])
