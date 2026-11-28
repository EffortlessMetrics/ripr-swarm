def parse(text, limit):
    if len(text) > limit:
        raise ValueError(
            f"Bad level: {text!r} is longer than {limit}"
        )
    return text.upper()
