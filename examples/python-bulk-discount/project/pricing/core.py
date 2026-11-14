def bulk_discount_rate(units):
    """Orders of 20 units or more get the bulk discount."""
    if units >= 20:
        return 0.15
    return 0.0
