<!-- section: Changed -->
- Rust checks: the stub-route lookup that `ripr check` and `ripr agent stub --at` run for a selected predicate finding now classifies only that finding's seam instead of every seam in its file. A one-line change in a large file no longer pays for re-classifying the whole file, and the output is unchanged (#13884).
