from fastapi.testclient import TestClient

from shop.app import app


def test_price_total():
    client = TestClient(app)
    response = client.get("/docs")
    assert response.json()["total"] == 195
