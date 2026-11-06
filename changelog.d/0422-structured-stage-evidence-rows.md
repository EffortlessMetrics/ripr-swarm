<!-- section: Added -->
- `ripr check --format json` findings carry the additive `evidence_stages`
  rows — stage, state, confidence, the capping basis each stage owns, and the
  contributing-test references behind the discriminate row — and `--format
  agent` / `ripr explain --format agent` print a compact `stages=` line per
  finding record, so scripts read the staged evidence without parsing prose;
  the human rendering is unchanged (RIPR-SPEC-0422, #12415).
