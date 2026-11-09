<!-- section: Changed -->
- Rust checks: the per-file identifier tallies kept for reach reasoning are packed into one buffer per file instead of a string per name, cutting `check` peak memory by about 6% with unchanged output (#13703).
