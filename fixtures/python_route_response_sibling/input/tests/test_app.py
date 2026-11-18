from fastapi.testclient import TestClient

from shop.app import app


def test_price_meta():
    client = TestClient(app)
    response = client.get("/price")
    assert response.json()["meta"] == {"count": 3}
