<!-- section: Fixed -->
- Rust: a `write!` edit that changes a format placeholder (`{}` to `{:?}` or `{0}`) is no longer read as a string-literal-only change, so an exact display pin is not credited for it; the ordered `{..}` groups inside the literals must match too (#9836, RIPR-SPEC-0297).
