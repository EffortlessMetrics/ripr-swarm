<!-- section: Fixed -->
- Rust checks: a same-file test helper that forwards the test's closure to
  another same-file function which runs it, then asserts, now lends that
  assertion to the test like a helper that invokes the closure itself. When
  the helper forwards the closure to a call ripr cannot see (tracing's
  `with_default(subscriber, fun)`), the test is no longer reported as having
  no assertion: its row reads `assertion_admission_unproven` and the finding
  names the helper and callee, with no change to oracle credit or class
  (#9981).
