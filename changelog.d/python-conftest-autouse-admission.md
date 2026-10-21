<!-- section: Fixed -->
- A Python test is no longer reported as having no assertion when a `conftest.py` above it
  could assert around it. A `conftest.py` on the test's directory chain that declares an
  `autouse` fixture, a `pytest_runtest*` or item-mutating hook, `pytest_plugins`, a name imported
  from a non-stdlib module, a decorator or fixture alias, or an autouse-fixture factory (or one
  ripr could not read) now leaves the test `assertion_like_present_but_unresolved`; a conftest in a sibling
  or child directory does not. Three single-file gaps close too: a same-module decorator that
  registers what it decorates in a global, registries filled in a module-level `while`,
  `match` or `except` body, and relative imports in a test file at the repository root.
  Verdicts are unchanged (#6657).
