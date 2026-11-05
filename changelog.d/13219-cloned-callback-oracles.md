<!-- section: Fixed -->
- Rust tests that pass a cloned callback to a helper that never invokes it no
  longer borrow the helper's unrelated assertion. Ordinary value clones and
  eagerly invoked literal callbacks keep their existing oracle credit (#13219).
