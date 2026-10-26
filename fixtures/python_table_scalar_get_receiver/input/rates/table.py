SCALAR_TABLE = {"gold": 10, "silver": 6}

def price(tier):
    return tier.get("silver", SCALAR_TABLE["silver"])
