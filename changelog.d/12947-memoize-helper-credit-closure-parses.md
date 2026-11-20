<!-- section: Changed -->
- Rust checks: memoize the closure-argument-position and invoked-callback syntax reads of the helper-credit pass within one run, so overlapping index builds skip re-parsing the same test and helper bodies (#12947).
