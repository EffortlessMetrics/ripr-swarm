def reject_negative(amount: int) -> int:
    if amount < 0:
        raise ValueError("negative amount")
    return amount
