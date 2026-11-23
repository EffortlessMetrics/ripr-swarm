def rate_for(region):
    return 1


def tax_for(amount, region):
    return amount * rate_for(region)
