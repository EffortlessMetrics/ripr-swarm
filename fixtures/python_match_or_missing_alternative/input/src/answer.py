def to_bool(val):
    match val:
        case "yes":
            return True
        case _:
            return False
