from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()


class dict(BaseModel):
    total: int


@app.get("/price")
def price() -> dict:
    return {"total": 1, "count": 3}
