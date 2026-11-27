def expired(
    now: int,
    max_age: int | None = None,
    strict: bool = False,
) -> int:
    age = now - 100
    if age >= max_age:
        return 1
    return 0
