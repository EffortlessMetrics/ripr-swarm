<!-- section: Fixed -->
- Python diff mode no longer drops a changed file whose path sorts past the `RIPR_PYTHON_MAX_WORKSPACE_FILES` cap. Changed `.py` files are kept past the file-count cap, so only unchanged context is refused there, and the `diff_scope_oversized` detail says so. The byte budgets and read failures still apply to changed files and stay disclosed (#10461).
