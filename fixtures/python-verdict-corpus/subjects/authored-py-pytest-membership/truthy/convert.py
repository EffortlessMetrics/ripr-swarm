"""Boolean spellings read from configuration text (#15632)."""


def to_bool(val):
    """Map a configuration spelling to a bool."""
    if isinstance(val, str):
        val = val.lower()

    if val in (True, "true", "t", "yes", "y", "on", "1", 1):
        return True
    if val in (False, "false", "f", "no", "n", "off", "0", 0):
        return False

    raise ValueError(val)
