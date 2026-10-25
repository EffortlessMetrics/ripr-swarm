def test_orders_respond(client):
    assert client.get("/orders").json() is not None
