<!-- section: Fixed -->
- Python: the `unresolved_pytest_fixture` next step no longer tells you to
  define a fixture that already exists in `conftest.py`. It now says to pass
  the input inline and that a fixture is read only when its whole body is a
  supported shape. A helper called as a module attribute (`conftest.helper(...)`
  after `from . import conftest`, or through `import pkg.conftest as c`) is
  followed like `from M import helper`. The read is refused, and the finding
  stays unresolved, when the alias or the helper can be rebound or patched:
  another import or `del` of the alias, a star import, `patch` or `setattr`,
  an assignment of the helper name, or a package `__init__` that binds the
  name (#13727).
