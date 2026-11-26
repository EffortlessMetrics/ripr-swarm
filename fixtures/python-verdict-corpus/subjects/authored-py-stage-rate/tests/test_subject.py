from src.subject import tax_for


def test_tax_for(monkeypatch):
    monkeypatch.setattr("src.subject.rate_for", lambda r: 2)
    assert tax_for(10, "x") == 20
