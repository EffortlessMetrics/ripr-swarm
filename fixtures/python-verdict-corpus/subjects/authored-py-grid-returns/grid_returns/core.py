"""Return values, one function per test style."""


def greeting(name):
    return f"Hello, {name}!"


def initial(name):
    return name[:1]


def normalize(text):
    return text.strip()


def shout(text):
    return text.upper()


def banner(title):
    return f"== {title} =="


def full_name(first, last):
    return first + " " + last
