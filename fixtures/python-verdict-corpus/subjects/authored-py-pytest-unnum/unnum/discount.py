def apply_discount(amount: float, threshold: float) -> float:
    if amount >= threshold:
        return amount * 0.9
    return amount
