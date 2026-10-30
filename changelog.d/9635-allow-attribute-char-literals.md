<!-- section: Internal / CI -->
- `check-allow-attributes` now reads char literals (`')'`, `'"'`, escapes, multibyte) in attribute text, so a quoted paren or quote in a `reason` no longer desyncs the attribute span or comma split and hides a later suppression; the `expect(dead_code)` ledger rows now carry per-site reasons instead of shared boilerplate (#9635).
