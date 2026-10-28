def total(text):
    return int(text) + 1


def pair(text):
    return (int(text) + 1, 3)


def scaled(x):
    return x * 2.0


def widen(text):
    return [int(text) + 1]


def narrow(text):
    return [int(text) + 1]


def patched(text):
    return [int(text) + 1]


def mirror(text):
    return [int(text) + 1]


def signed(text):
    return -int(text) - 1


def wide(text):
    return int(text) + 0x100000000000000000000000000000000


def spaced(text):
    return [int(text) + 1]
