<!-- section: Internal / CI -->
- A seam-evidence pass that fails closed now hands its single pass-wide
  `producer_failure` limitation to the scoped (streamed and non-streamed) and
  cold full seam-inventory results, and refuses to describe an unbuildable
  failure silently, with no output, schema or golden change; healthy walks are
  unchanged. The uncached full walk, the targeted rerun and `classify_in_file`
  remain unplumbed and are tracked on #13340 (#13340).
