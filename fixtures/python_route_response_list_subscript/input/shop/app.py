from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": [10][0], "count": 3}
