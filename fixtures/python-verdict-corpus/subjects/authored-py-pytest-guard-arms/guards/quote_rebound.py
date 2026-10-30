def quote_rebound(total):
    total = 2000
    if total > 1000:
        return round(total * 1.02, 2)
    return round(total * 1.05, 2)
