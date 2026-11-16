<!-- section: Fixed -->
- `ripr repair` and the repo-wide seam view no longer count a guarded
  boundary (`amount >= LIMIT && !flag`) as gripped from a test that calls it
  with the guard missing (`flag = true`). They now agree with `ripr check`,
  which reads that change as unclear, and the check screen no longer prints
  a conflicting-evidence advisory for it (#14872).
