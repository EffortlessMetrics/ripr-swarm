<!-- section: Fixed -->
- `ripr repair` and the repo-wide seam view no longer count a guarded
  boundary (`amount >= LIMIT && !flag`) as gripped from a test that calls it
  with the guard missing (`flag = true`). They now agree with `ripr check`,
  which reads that change as unclear, and the check screen no longer prints
  a conflicting-evidence advisory for it (#14872).
- `ripr check` and the repo-wide seam view now read a boundary guard that
  rustfmt wraps onto its own line (`if amount >= LIMIT` then `&& !flag`) the
  same as a one-line guard (#14872).
