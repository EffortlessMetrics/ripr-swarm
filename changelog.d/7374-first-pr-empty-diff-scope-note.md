<!-- section: Fixed -->
- When the review-cards artifact was rendered from an empty `base..head` diff
  (work committed on the base branch itself) while the gap ledger still carries
  repo-scoped rows, the first-pr no-action reason now names the scope mismatch
  and points at regenerating the cards with the real PR base (for example
  `--base HEAD~1`) instead of leaving a vacuous "no actionable gap" (#7374).
