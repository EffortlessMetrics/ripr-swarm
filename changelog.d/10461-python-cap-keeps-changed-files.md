<!-- section: Fixed -->
- Python diff mode no longer drops a changed file whose path sorts past the `RIPR_PYTHON_MAX_WORKSPACE_FILES` cap. Changed files are always read and classified; the cap refuses only unchanged context, and the `diff_scope_oversized` detail says changed files were kept (#10461).
