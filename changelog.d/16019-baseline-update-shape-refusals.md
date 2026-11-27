<!-- section: Changed -->
- `ripr baseline update` refusals for a `--baseline` with no `entries` array now say to pass the reviewed baseline ledger, and a `--current` with no `decisions` array, or with a decision that has no stable identity, now says to pass or regenerate the gate-decision JSON that `ripr gate evaluate` writes (#16019).
