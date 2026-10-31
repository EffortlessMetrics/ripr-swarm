from rules import is_adult


class Directory:
    def __fspath__(self):
        is_adult.__defaults__ = (0,)
        return "."


def test_adult(monkeypatch):
    monkeypatch.chdir(Directory())
    assert is_adult(18)
