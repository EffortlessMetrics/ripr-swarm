import math


def largest(xs):
    best = -math.inf
    for x in xs:
        if x >= best:
            best = x
    return best
