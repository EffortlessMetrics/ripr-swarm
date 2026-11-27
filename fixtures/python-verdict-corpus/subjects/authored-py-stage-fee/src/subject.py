def fee_for(tier):
    return 2


def total_for(amount, tier):
    return amount + fee_for(tier)
