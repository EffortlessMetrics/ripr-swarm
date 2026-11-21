def clamp(value, floor=None):
    if floor is not None and value <= floor:
        return f"<{floor}"
    return str(value)
