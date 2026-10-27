<!-- section: Fixed -->
- `ripr pilot` no longer prints `findings: 0` / `no_python_findings` for a Python repo with uncommitted Python edits: its Python preview now analyzes the same working-tree diff pilot's current change and printed `ripr check` command select, so the count matches `ripr check` (#10681).
