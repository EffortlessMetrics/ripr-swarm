<!-- section: Fixed -->
- `ripr check` no longer claims reach for a changed `async fn` whose only
  reaching tests create its future and drop it without `.await`, an executor
  or an async test attribute. Such a finding now reads `static_unknown` under
  the new `rust_future_never_polled` limitation ("future created but never
  polled"), with reach `unknown`; awaited and `block_on` calls keep their
  reading, and repair still does not start or choose a runtime (#13702).
