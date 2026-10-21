<!-- section: Internal / CI -->
- `cargo xtask termination-qualification --binary <path>` runs a deterministic
  adversarial corpus (Rust nesting shapes on `check` and `pilot`, Python deep
  operator, unary and `elif` chains, and many-star path globs through
  suppression selectors and `generated_file_patterns`) against one exact `ripr`
  binary. Each row runs in an owned disposable workspace with an unchanged
  hostile file outside the diff and an outer deadline, and ends in one #4505
  result state. One DTO renders the JSON and Markdown packet with the binary's
  SHA-256, per-case input digests, subject counts, cleanup state and negative
  controls. No product behavior changes (#4505, #4510, #4511, #4512, #4513).
