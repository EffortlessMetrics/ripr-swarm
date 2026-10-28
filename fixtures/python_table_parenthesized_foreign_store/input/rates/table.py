PAREN_TABLE = ({"gold": 10, "silver": 6})

def price(tier):
    return PAREN_TABLE.get(tier, 0)
