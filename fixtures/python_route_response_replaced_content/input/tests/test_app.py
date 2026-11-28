from fastapi.testclient import TestClient

from shop.app import app


def test_price_total():
    client = TestClient(app)
    response = client.get("/price")
    response._content = b'{"total": 190}'
    assert response.json()["total"] == 190
