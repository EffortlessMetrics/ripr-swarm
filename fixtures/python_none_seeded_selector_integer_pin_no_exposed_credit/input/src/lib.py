def largest(xs):
    best = None
    for x in xs:
        if best is None or x >= best:
            best = x
    return best
