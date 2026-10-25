<!-- section: Fixed -->
- Python: a logged-text check is credited only when `caplog` is pytest's own fixture; a test that does not request it, or whose module or `conftest.py` redefines it, abstains instead of being credited (#9546).
