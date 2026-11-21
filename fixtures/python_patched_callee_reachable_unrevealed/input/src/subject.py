def rate_for(region):
    return 3


def tax_for(amount, region):
    return amount * rate_for(region)
