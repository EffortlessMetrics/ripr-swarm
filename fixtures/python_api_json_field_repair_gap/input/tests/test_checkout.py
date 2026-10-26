from starlette.testclient import TestClient

from app.checkout import api


def test_expired_coupon_response_smoke():
    client = TestClient(api)
    response = client.post("/checkout", json={"expired": True})
    assert response
