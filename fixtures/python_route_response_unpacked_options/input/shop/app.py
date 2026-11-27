from fastapi import FastAPI

app = FastAPI(**{"openapi" "_url": "/price"})


@app.get("/price")
def price():
    return {"openapi": "3.1.0", "count": 3}
