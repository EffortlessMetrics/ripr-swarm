METHOD_TABLE = {"gold": 10, "silver": 6}

def price(tier):
    return METHOD_TABLE.get(tier, 0)
