<!-- section: Fixed -->
- The Python verify route for a test file inside a nested project (for example
  `py/` with its own `pyproject.toml` in a monorepo) now runs from that project
  directory with the test path and any project `.venv` interpreter relative to
  it, instead of from the repository root where pytest cannot import the
  project's package. Interpreter and confidence come from that same directory;
  a nested project with no venv under a root that has one is `medium`, not
  `high`. The human card, JSON (`verify_cwd`, `verify.cwd`), SARIF, GitHub
  annotation, gap ledger, first-pr command context, pilot and agent packets
  carry the one directory, and first-pr withholds the verify form when it
  cannot resolve it. Only a directory that declares a project (`setup.py`, or
  a `pyproject.toml` / `setup.cfg` packaging section) counts; a lint-only
  `setup.cfg` in `tests/` does not. Root-level projects are unchanged
  (#15302).
