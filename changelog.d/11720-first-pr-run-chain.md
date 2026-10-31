<!-- section: Added -->
- `ripr first-pr --run-chain` is an opt-in that runs the missing-artifact chain
  (repo-exposure report, gap ledger, review cards) itself and composes the
  packet in one invocation, using exactly the commands `first-pr` prints with
  the resolved base and head. The first failing step stops it with exit 2 and
  no packet. The default stays analysis-free with an unchanged disclosure
  (#11720, #11298).
