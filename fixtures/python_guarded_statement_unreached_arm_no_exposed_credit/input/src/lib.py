def describe(kind, n):
    return f"{n} {kind}"


def humanize(diff, months):
    if diff < 10:
        return "just now"
    elif months >= 1 and diff < 1000:
        m = 2 * months
        return describe("months", m)
    return describe("years", diff)
