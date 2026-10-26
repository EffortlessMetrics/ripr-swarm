MISSING_TABLE = {1: 5, 2: 6}

def price(tier):
    return MISSING_TABLE.get(tier, MISSING_TABLE[2])
