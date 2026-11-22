def largest(xs):
    best = float("-inf")
    for x in xs:
        if x >= best:
            best = x
    return best
