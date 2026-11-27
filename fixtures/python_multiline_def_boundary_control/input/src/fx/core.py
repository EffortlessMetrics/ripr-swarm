def over(
    age: int,
    max_age: int | None = None,
) -> int:
    if age >= max_age:
        return 1
    return 0
