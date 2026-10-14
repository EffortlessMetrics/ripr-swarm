<!-- section: Fixed -->
- Doctor: `ripr doctor` now reports `python: pytest` for a zero-config
  pytest repo whose `test_*.py` files import pytest, matching `ripr check`.
  Config-file markers (`pytest.ini`, `conftest.py`, pytest sections) still
  win first (#7662).
