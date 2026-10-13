<!-- section: Fixed -->
- `ripr agent stub --json` now emits the versioned `rust_test_stub`
  `state: "refused"` envelope (kind `seam_not_found`) on stderr when the
  selector names no reported gap, matching the documented exit-code contract
  and the post-resolution refusal shape. Exit code (3) and empty stdout are
  unchanged (#7390).
