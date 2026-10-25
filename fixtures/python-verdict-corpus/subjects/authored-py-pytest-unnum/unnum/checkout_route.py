from types import SimpleNamespace


class Api:
    def post(self, path):
        def register(func):
            return func

        return register


api = Api()
response = SimpleNamespace(status_code=200)


@api.post("/checkout")
def checkout():
    response.status_code = 422
