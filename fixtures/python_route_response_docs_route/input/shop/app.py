from fastapi import FastAPI

app = FastAPI()


@app.get("/docs")
def price():
    return {"total": 195, "count": 3}
