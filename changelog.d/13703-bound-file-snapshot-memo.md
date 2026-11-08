<!-- section: Changed -->
- Rust checks: the run-wide cache of whole-file parse snapshots now keeps at most 24 MiB of source resident (least recently used first out), cutting peak memory on large workspaces by about 11% with unchanged output and speed (#13703).
