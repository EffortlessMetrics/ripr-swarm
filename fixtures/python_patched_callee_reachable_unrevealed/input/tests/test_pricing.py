from pricing import tax_for


def test_tax_for(monkeypatch):
    monkeypatch.setattr("pricing.rate_for", lambda region: 2)
    assert tax_for(10, "x") == 20
