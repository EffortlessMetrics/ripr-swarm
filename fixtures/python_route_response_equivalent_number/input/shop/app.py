from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": [5.0], "count": 3}
