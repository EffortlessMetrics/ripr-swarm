<!-- section: Internal / CI -->
- The required Rust lane now runs strict Clippy for all-feature RIPR targets
  in place of the existing all-features type check, while retaining the
  default-feature workspace lint pass for disabled-feature branches (#13152).
