<!-- section: Internal / CI -->
- Removed the nextest test `test_covered_by_production_wrapper_enumerates_through_cargo` (~108 s), which repeated the real-Cargo covered_by enumeration that `cargo xtask check-file-policy` already runs in the blocking Repository policy preflight; the cheap failure-path pin stays and the slow-test baseline allowlist drops the entry (#12916).
