def is_musl(tag):
    if tag[:3] != "mus":
        return False
    return True
