from idioms.env import home_dir


def test_home_dir_reads_the_environment(monkeypatch):
    monkeypatch.setenv("HOME_DIR", "/h")
    assert home_dir() == "/h"
