<!-- section: Added -->
- `ripr first-pr --run-chain` is an opt-in that runs the missing-artifact chain
  (repo-exposure report, gap ledger, review cards) itself and composes the
  packet in one invocation, using exactly the commands `first-pr` prints with
  the resolved base and head (the review cards only when the gap ledger names
  no repair start). The first failing step, or a step that exits 0 without a
  non-empty, parseable JSON artifact, stops it with exit 2 and no packet. The default stays analysis-free with an unchanged disclosure
  (#11720, #11298).
