
class Api:
    def post(self, path):
        def decorator(func):
            return func

        return decorator


app = Api()


@app.post("/checkout")
def admin_checkout(payload):
    return {"detail": "admin ok"}
