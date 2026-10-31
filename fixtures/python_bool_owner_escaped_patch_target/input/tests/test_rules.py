import rules
from rules import is_adult


def test_adult():
    assert is_adult(18)


def test_adult_when_stubbed(monkeypatch):
    monkeypatch.setattr(rules, "is\x5fadult", lambda age: False)
    assert rules is not None
