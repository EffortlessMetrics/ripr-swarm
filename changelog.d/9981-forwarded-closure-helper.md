<!-- section: Fixed -->
- Rust checks: a same-file test helper that forwards the test's closure to
  another same-file function which runs it, then asserts, now lends that
  assertion to the test like a helper that invokes the closure itself. When
  a helper with a recognized assertion (such as `assert_eq!`) after the
  forward hands the closure to a call ripr cannot see (for example
  `tracing::subscriber::with_default(subscriber, fun)`), the test is no
  longer reported as having no assertion: its row reads
  `assertion_admission_unproven`, the finding names the helper and callee,
  and when those are the only assertions the related tests hold, the finding
  gives the existing unestablished-assertion next step instead of "Add a
  meaningful assertion". Oracle credit and class are unchanged. A helper
  whose only check is a method such as `handle.assert_finished()` is not yet
  covered (#9981).
