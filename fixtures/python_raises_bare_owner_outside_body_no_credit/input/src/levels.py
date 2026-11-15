def parse(text):
    if not text:
        raise ValueError("blank level")
    return text.upper()


def other(text):
    raise ValueError("blank level")

