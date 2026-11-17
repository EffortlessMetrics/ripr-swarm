<!-- section: Fixed -->
- Rust checks: a local test-generator macro that calls itself or a standard assertion in its body (httparse's `req!`) no longer leaves every `assert_eq!` in its file uncredited, and plain test functions between its calls no longer block the exemption (#14761).
