def quote(total):
    if total > 1000:
        return 0
    return round(total * 1.10, 2)
