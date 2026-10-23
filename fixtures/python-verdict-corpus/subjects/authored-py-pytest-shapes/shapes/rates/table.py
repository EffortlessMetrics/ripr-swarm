RATE_TABLE = {"gold": 10, "silver": 5}


def lookup(tier):
    return RATE_TABLE.get(tier, 0)
