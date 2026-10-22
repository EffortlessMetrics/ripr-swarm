def slugify(text):
    """Lowercase and join words with dashes.

    >>> slugify("Hello World")
    'hello-world'
    """
    return "-".join(text.lower().split())


def truncate(text, limit):
    """Cut text to at most `limit` characters.

    >>> truncate("abc", 5)
    'abc'
    """
    return text[:limit]


def shout(text):
    return text.upper() + "!"
