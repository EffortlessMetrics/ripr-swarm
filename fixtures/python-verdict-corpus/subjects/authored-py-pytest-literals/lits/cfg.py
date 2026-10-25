def server_config():
    return {
        "host": "localhost",
        "retries": 3,
    }


def ports():
    return {
        "http": 1,
        "admin": 8080,
    }


def limits():
    return {
        "low": 10,
        "high": 10,
    }


def whole_config():
    return {
        "mode": "fast",
        "level": 2,
    }


def roles():
    return [
        "admin",
        "user",
    ]


def staff():
    return [
        "owner",
        "editor",
    ]


def timeouts():
    return {
        "connect": 5,
        "read": 30,
    }


def labels():
    return {
        "name": "x",
        "tag": "y",
    }
