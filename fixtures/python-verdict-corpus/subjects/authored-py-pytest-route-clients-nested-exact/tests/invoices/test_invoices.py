def test_invoices_total(client):
    assert client.get("/invoices").json()["total"] == 30
