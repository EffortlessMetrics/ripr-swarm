<!-- section: Changed -->
- Python diff mode now streams the workspace walk into a bounded selection instead of collecting and sorting every discovered `.py` path first. Discovery memory stays at the `RIPR_PYTHON_MAX_WORKSPACE_FILES` cap plus the changed files, however large the workspace. The analyzed set and the refused count are unchanged (#10594).
