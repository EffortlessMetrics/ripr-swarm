from fastapi import FastAPI

app = FastAPI()


@app.get("/price")
def price():
    return {"total": 3.3 // 0.3, "count": 3}
