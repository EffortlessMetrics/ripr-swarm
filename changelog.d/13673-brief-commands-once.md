<!-- section: Changed -->
- `ripr agent brief --json` (schema `0.2`) states the before/after snapshot
  commands once under `next` instead of repeating them in every top seam's
  `verification` block, and drops the per-seam `verify_command` (identical to
  `next.verify_after_edit`). `verification.suggested_test_command` is
  unchanged. Readers of `top_seams[].verification.{before,after}_snapshot_command`
  and `verify_command` should read `next.*` (#13673).
