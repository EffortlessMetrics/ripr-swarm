<!-- section: Added -->
- `ripr check --format json` findings carry the additive `evidence_stages`
  rows — stage, state, confidence and the capping basis each stage owns —
  and `--format agent` / `ripr explain --format agent` print a compact
  `stages=` line per finding record, so scripts read the staged evidence
  without parsing prose; the human rendering is unchanged and no row claims
  test attribution (RIPR-SPEC-0429, #13453).
