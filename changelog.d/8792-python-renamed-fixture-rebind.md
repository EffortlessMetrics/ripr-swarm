<!-- section: Fixed -->
- Python: a later `@pytest.fixture(name=...)` def of the same function name drops the earlier fixture, matching pytest's module-attribute rebind, so `fixture_owner_call` no longer follows the stale name (#8792).
