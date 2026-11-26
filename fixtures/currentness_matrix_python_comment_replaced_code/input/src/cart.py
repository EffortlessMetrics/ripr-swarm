CAP = 100


def capped_total(items):
    total = sum(items)
    # The cap moved to the caller.

    return total


def clamped_double(x):
    # one clamp for both ends
    return max(min(x * 2, CAP), 0)
