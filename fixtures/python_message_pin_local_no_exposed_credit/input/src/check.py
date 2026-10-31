def check(x):
    if x < 0:
        msg = "negative value: must be at most zero"
        raise ValueError(msg)
    return x
