def quote_high(total):
    if total > 1000:
        return round(total * 1.02, 2)
    return round(total * 1.05, 2)
