<!-- section: Security -->
- ripr no longer reads a `ripr.toml` that is a symbolic
  link leading outside the repository being analyzed. `check`, `diff`, `pilot`
  and the structured-output routes stop with one fixed message naming the entry
  and how to repair it, before the link target is opened. A link whose target
  stays inside the repository still loads as before (#14310).
