from types import SimpleNamespace

response = SimpleNamespace(status_code=422)


def checkout():
    response.status_code = 400
