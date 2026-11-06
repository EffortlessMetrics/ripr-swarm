import pytest


@pytest.fixture(autouse=True)
def _quiet_env(monkeypatch):
    monkeypatch.setenv("TZ", "UTC")
