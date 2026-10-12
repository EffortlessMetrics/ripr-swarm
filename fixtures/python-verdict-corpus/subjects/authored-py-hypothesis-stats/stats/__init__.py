"""Small numeric helpers tested with Hypothesis properties."""


def clamp(x, lo, hi):
    """Limit x to the closed range [lo, hi]."""
    return max(lo, min(x, hi))


def absolute(x):
    """Absolute value of an integer."""
    return x if x >= 0 else -x


def mean(xs):
    """Arithmetic mean of a non-empty list."""
    return sum(xs) / len(xs)


def is_passing(score):
    """A score of 50 or more passes."""
    return score >= 50


def grade_label(score):
    """Display label for a score."""
    return "pass" if is_passing(score) else "fail"
