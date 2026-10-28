BOOL_TABLE = {1: 5, 2: 6}

def price(tier):
    return BOOL_TABLE.get(tier, BOOL_TABLE[2])
