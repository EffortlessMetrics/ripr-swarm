<!-- section: Changed -->
- Rust checks: call facts decoded from the fact cache share one allocation per source line again, as freshly extracted facts do, which cuts warm-run peak memory (#13703).
