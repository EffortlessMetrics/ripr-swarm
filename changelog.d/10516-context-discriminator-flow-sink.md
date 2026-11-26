<!-- section: Changed -->
- `ripr context --json` now writes `flow_sink` on each top-level
  `missing_discriminators` entry, with the same field and shape that the
  `ripr check --json` finding carries (`{kind, text, line}` or `null`). A test
  holds every packet's array equal to its check finding's array. The field is
  additive, so the packet `version` stays `1.1` (#10516).
