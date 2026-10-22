<!-- section: Internal / CI -->
- `cargo xtask termination-qualification --binary <path>` runs a deterministic
  Rust nesting and Python deep-syntax corpus against one exact `ripr` binary,
  each row in an owned disposable workspace with an outer deadline and one
  #4505 result state, and writes a JSON and Markdown packet with the binary
  SHA-256, input digests, cleanup state and negative controls. No product
  behavior changes (#4505, #4510, #4511, #4512).
