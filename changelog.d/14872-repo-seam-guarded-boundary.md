<!-- section: Fixed -->
- `ripr repair` and the repo-wide seam view no longer count a guarded
  boundary (`amount >= LIMIT && !flag`) as gripped from a test that calls it
  with the guard missing (`flag = true`). They now agree with `ripr check`,
  which reads that change as unclear, and the check screen no longer prints
  a conflicting-evidence advisory for it (#14872).
- `ripr check` and the repo-wide seam view now read a boundary guard that
  rustfmt wraps onto its own line (`if amount >= LIMIT` then `&& !flag`) the
  same as a one-line guard (#14872).
- Bare `ripr repair` on a branch no longer starts a lone repair whose change
  `ripr check` reads as already noticed or cannot judge; it says why, the
  same as when several repairs are possible (#14872).
