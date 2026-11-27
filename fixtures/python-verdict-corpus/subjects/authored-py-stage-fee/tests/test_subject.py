from src.subject import total_for


def test_total_for(monkeypatch):
    monkeypatch.setattr("src.subject.fee_for", lambda t: 7)
    assert total_for(100, "gold") == 107
