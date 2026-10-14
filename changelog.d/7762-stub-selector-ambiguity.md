<!-- section: Changed -->
- `ripr agent stub --json` moves `rust_test_stub` from schema `0.1` to
  `0.2`, retaining its fields and ready/refused states. Ambiguous file or
  tied-seam selectors now refuse as `selector_ambiguous`; genuinely absent
  selectors remain `seam_not_found`. Consumers must accept `0.2` and route
  ambiguity to path/seam-ID disambiguation. Exit 3, empty stdout, human
  guidance, and no-write refusals are unchanged (#7762).
