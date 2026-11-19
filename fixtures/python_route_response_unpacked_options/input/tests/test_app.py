from fastapi.testclient import TestClient

from shop.app import app


def test_price_schema_version():
    client = TestClient(app)
    response = client.get("/price")
    assert response.json()["openapi"] == "3.1.0"
