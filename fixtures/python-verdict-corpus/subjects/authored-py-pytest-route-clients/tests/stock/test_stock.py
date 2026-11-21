def test_stock_route(api_client):
    assert api_client.get("/stock").json() == {"left": 5}
