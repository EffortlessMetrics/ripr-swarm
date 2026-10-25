def check(x):
    if x < 0:
        raise ValueError("negative value: must be at most zero")
    return x
