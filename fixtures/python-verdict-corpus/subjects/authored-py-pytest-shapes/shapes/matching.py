"""Structural pattern matching."""


def kind(value):
    match value:
        case 0:
            return "zero"
        case int():
            return "int"
        case _:
            return "other"
