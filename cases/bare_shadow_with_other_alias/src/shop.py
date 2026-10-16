def fee(amount):
    if amount < 0:
        raise ValueError('negative')
    return amount * 3
