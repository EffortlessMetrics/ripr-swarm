RATES = {"silver": 6}

def price(tier):
    return RATES.get(tier, 0)
