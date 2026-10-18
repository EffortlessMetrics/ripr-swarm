<!-- section: Fixed -->
- Python: a test that receives the owner result through a pytest fixture (same module or `conftest.py`) is now related with a weak `fixture_owner_call` relation instead of `no_static_path` (#4563); the relation carries no oracle.
