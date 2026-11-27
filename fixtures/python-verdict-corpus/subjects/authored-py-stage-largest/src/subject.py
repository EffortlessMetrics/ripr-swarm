def largest(xs):
    it = iter(xs)
    try:
        best = next(it)
    except StopIteration:
        return None
    for x in it:
        if x > best:
            best = x
    return best
