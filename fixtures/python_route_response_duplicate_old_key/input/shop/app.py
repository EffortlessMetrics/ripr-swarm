from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": 190, "count": 3}
