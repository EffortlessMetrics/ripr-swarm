<!-- section: Added -->
- `ripr check --format json-compact` emits the same structured static report
  as `json` with insignificant whitespace removed and one final newline.
  Findings, provenance, actions, limitations, suppression policy, structured
  refusals and exit status retain their existing meaning; the findings budget
  is applied before compaction, and `--json` remains unchanged (#13103).
