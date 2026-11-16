<!-- section: Changed -->
- Rust checks: the assertion-context gate proves a test file's `std`/`core` assertion roots only for the rooted spellings that file calls, and proves any other spelling once on demand, so reading a file no longer runs 24 whole-file root proofs; findings are unchanged (#13703).
