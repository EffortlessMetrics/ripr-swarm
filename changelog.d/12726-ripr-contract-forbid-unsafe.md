<!-- section: Security -->
- `ripr-contract` now carries `#![forbid(unsafe_code)]` in its crate root, so
  the unsafe ban survives standalone/published builds that do not inherit the
  workspace lint table (#12726).
