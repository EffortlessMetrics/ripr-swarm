<!-- section: Fixed -->
- `ripr calibrate` now states the counting unit of every static/runtime
  agreement bucket (per-seam vs per-record) and reconciles
  `runtime_signal_without_static_gap` against `unmatched_mutants_total` when
  unmatched records contribute, so the bucket column no longer reads as an
  arithmetic bug against `matched_total` (#7742).
