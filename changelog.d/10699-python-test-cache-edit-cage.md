<!-- section: Fixed -->
- The repair edit cage no longer reports Python test-run cache output
  (`__pycache__` bytecode, `.pytest_cache`, `.mypy_cache`, `.ruff_cache` and
  root `.coverage` data) as an unauthorized edit; tracked files, importable
  source and baselines saved by older binaries keep their meaning (#10699).
