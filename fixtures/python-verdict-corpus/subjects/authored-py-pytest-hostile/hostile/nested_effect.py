def collect():
    values = []

    def inner():
        return values.append(2)

    inner()
    return values
