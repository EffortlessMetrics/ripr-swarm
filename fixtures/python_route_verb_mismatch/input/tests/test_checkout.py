def test_expired_coupon_get_does_not_reach_post_route(client):
    response = client.get("/checkout")
    assert response.json()["detail"] == "coupon expired"
