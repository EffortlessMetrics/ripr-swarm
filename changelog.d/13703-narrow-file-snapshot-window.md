<!-- section: Changed -->
- Rust checks: the run-wide cache of whole-file parse snapshots now keeps at most 8 MiB of source resident instead of 24 MiB, cutting peak memory on this repository's own check by about 9% (1.41 GB to 1.28 GB) with unchanged output and speed (#13703).
