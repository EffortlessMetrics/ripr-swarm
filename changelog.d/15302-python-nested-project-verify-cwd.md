<!-- section: Fixed -->
- The Python verify route for a test file inside a nested project (for example
  `py/` with its own `pyproject.toml` in a monorepo) now runs from that project
  directory with the test path and any project `.venv` interpreter relative to
  it, instead of from the repository root where pytest cannot import the
  project's package. The human card, JSON (`suggested_verify_cwd`,
  `verify_cwd`, `verify.cwd`), SARIF, GitHub annotation and first-pr command
  context all carry the one directory; root-level projects are unchanged
  (#15302).
