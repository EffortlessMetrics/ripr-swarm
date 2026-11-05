import builtins as b

b.isinstance += 1


def label(x):
    if isinstance(x, str):
        return "TEXT"
    return None
