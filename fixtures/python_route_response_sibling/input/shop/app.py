from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": 200 - 5, "meta": {"count": 3}}
