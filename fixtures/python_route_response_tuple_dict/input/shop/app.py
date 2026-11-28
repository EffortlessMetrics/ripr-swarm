from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()


class Price(BaseModel):
    total: int


dict, unused = Price, None


@app.get("/price")
def price() -> (dict):
    return {"total": 1, "count": 3}
