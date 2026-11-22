import warnings


def headers(env):
    warnings.warn(
        "headers() is deprecated",
        DeprecationWarning,
        stacklevel=3,
    )
    return {"a": env}
