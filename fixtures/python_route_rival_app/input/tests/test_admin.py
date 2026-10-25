from shop.admin import app
from starlette.testclient import TestClient


def test_admin_checkout_reports_ok():
    client = TestClient(app)
    response = client.post("/checkout", json={})
    assert response.json()["detail"] == "admin ok"
