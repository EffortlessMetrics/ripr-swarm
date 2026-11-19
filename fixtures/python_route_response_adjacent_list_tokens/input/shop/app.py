from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": [2 +3], "count": 3}
