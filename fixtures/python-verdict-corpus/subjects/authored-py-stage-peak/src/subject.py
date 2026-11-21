def peak(xs):
    best = xs[0]
    for x in xs:
        if x > best:
            best = x
    return best
