def quote_force(total, force=False):
    if total > 1000 or force:
        return round(total * 1.02, 2)
    return round(total * 1.05, 2)
