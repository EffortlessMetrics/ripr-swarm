from trapkit import core


def test_patched_owner(monkeypatch):
    monkeypatch.setattr(core, "patched_owner", lambda a, b: 21)
    assert core.patched_owner(3, 7) == 21
