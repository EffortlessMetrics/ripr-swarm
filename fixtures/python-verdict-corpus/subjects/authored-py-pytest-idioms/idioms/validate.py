def parse_age(text):
    age = int(text)
    if age < 0:
        raise ValueError("age must be non-negative")
    return age
