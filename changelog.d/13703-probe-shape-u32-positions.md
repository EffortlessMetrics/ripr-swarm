<!-- section: Changed -->
- Rust checks: each parsed probe site now stores its line numbers and byte offsets as 32-bit values, cutting peak memory on this repository's own check by about 3% (1.20 GB to 1.16 GB) with unchanged output and speed (#13703).
