<!-- section: Changed -->
- `ripr check` is faster on macro-heavy workspaces: the workspace macro-binding scan looks up a file's imported names once instead of walking every `use` tree for each macro call. Findings are unchanged (#11613).
