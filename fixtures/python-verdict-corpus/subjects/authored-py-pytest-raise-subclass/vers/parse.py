class InvalidVersion(ValueError):
    pass


def parse_version(text):
    if not text.strip():
        raise ValueError("invalid version")
    return int(text)


def parse_strict(text):
    if not text.strip():
        raise InvalidVersion("invalid version")
    return int(text)
