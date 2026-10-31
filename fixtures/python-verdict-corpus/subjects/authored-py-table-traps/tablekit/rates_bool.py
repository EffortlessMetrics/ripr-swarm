BOOL_RATES = {1: 5, 2: 5}


def price(tier):
    return BOOL_RATES.get(tier, BOOL_RATES[2])
