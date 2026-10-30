def label(n):
    def inner(m):
        return f"#{m}"

    return inner(n)
