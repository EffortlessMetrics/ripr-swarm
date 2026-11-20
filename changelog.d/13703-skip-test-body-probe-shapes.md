<!-- section: Changed -->
- Rust checks: probe sites inside plain `#[test]` function bodies are no longer stored, because no finding reads them. On this repository's own check, peak memory drops by about 2.5% (1.12 GB to 1.09 GB) and the per-file facts cache by about 11%, with unchanged output. The cache schema moves to 1.62, so the first run after upgrading re-reads each file once (#13703).
