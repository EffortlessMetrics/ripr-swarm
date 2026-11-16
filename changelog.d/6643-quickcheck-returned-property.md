<!-- section: Fixed -->
- Rust: a `#[quickcheck]` property that returns `bool` or `TestResult` is no
  longer reported as a test with "no assertion" with advice to add one. Its
  returned value is the check; ripr now names that it does not read it,
  without granting credit (#6643).
