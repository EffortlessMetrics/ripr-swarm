<!-- section: Fixed -->
- Python: a test file that calls `importlib.reload`, `__setattr__` or `delattr` (including through a renamed `monkeypatch` fixture) is now treated as a dynamic namespace writer, so module constants and module-object field writes it may replace are no longer credited (#10184).
