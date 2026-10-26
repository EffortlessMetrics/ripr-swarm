def test_invoices_respond(client):
    assert client.get("/invoices").json() is not None
