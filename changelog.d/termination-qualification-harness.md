<!-- section: Internal / CI -->
- `cargo xtask termination-qualification --binary <path>` runs a deterministic
  adversarial corpus (Rust nesting shapes on `check` and `pilot`, Python deep
  operator, unary and `elif` chains, and many-star path globs through
  suppression selectors and `generated_file_patterns`) against one exact `ripr`
  binary, each row in an owned disposable workspace with an outer deadline and
  one #4505 result state, and writes a JSON and Markdown packet with the binary
  SHA-256, input digests, cleanup state and negative controls. No product
  behavior changes (#4505, #4510, #4511, #4512, #4513).
