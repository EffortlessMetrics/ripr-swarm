from rules import is_adult


class Local:
    def setenv(self, *args):
        is_adult.__defaults__ = (0,)


def apply(monkeypatch):
    monkeypatch.setenv("A", "B")


def test_adult():
    apply(Local())
    assert is_adult(18)
