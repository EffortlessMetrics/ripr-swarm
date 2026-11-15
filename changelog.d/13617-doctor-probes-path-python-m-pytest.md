<!-- section: Changed -->
- Python doctor: probe `python -m pytest` (the command the verify route prints) from a private empty directory instead of a standalone `pytest` binary, warn when pytest is not importable, and hint at activating a project `.venv`/`venv` that has pytest when PATH `python` lacks it (#13617, #13799).
