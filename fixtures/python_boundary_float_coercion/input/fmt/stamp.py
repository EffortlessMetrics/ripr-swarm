def seconds(value):
    value = float(value)
    if value <= 0:
        raise ValueError("not a positive timestamp")
    return value
