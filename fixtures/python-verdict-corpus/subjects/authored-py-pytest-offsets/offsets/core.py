LIMIT = 5


def tier_a(n):
    if n > LIMIT - 1:
        return "high"
    return "low"


def tier_b(n):
    if n < LIMIT + 1:
        return "low"
    return "high"


def tier_c(n):
    if n > LIMIT - 1:
        return "high"
    return "low"


def tier_d(n, threshold):
    if n > threshold - 1:
        return "high"
    return "low"


def tier_e(n, threshold):
    if n > threshold - 1:
        return "high"
    return "low"
