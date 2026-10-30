PLAIN_TABLE = ({"gold": 10, "silver": 6})

def price(tier):
    return PLAIN_TABLE.get(tier, 0)
