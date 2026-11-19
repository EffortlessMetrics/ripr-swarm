from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": 195, "count": 3}
