"""A route registry and request client small enough to run without a web framework."""


class MiniApp:
    def __init__(self):
        self.routes = {}

    def get(self, path):
        def register(func):
            self.routes[path] = func
            return func

        return register


class Response:
    def __init__(self, body):
        self._body = body

    def json(self):
        return self._body


class TestClient:
    __test__ = False

    def __init__(self, app):
        self.app = app

    def get(self, path):
        return Response(self.app.routes[path]())
