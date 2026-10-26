
class Api:
    def post(self, path):
        def decorator(func):
            return func

        return decorator


app = Api()


@app.post("/checkout")
def checkout(payload):
    if payload.get("expired"):
        return {"detail": "coupon expired"}
    return {"detail": "ok"}
