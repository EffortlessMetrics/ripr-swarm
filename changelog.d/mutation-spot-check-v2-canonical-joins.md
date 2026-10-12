<!-- section: Changed -->
- `cargo xtask mutation-spot-check` now writes `ripr-mutation-spot-check-v2`.
  It scores only `canonical_precise` records: operator mutants that
  `ripr calibrate cargo-mutants` joined by `seam_id` or span containment to a
  predicate or return seam. Every other record carries a named exclusion
  (`file_line_only`, `ambiguous_span_overlap`, `unmatched_<reason>`,
  `unsupported_genre`, ...). The v1 operator-text match survives only as a
  diagnostic. On atuin `90f590b9`, 7 of 27 mutants score instead of 1. The DX
  scoreboard reads v2 and refuses v1 receipts (#5487).
