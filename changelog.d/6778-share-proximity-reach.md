<!-- section: Changed -->
- Rust diff checks reuse the run-level proximity reach graph across findings,
  avoiding repeated graph setup for changed match arms (#6778).
