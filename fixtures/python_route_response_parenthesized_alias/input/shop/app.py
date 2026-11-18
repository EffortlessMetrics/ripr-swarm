from fastapi import FastAPI

app = FastAPI()


def cached():
    return {"total": 190, "count": 3}


register = (
    app.get("/price")
)
register(cached)


@app.get("/price")
def price():
    return {"total": 195, "count": 3}
