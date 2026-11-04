<!-- section: Internal / CI -->
- The TypeScript preview repair-loop dogfood corpus retains a positive
  `already_observed_unchanged` case and `must_not_claim_already_observed` guards
  that fail closed when live preview evidence spuriously promotes to
  `already_observed` (#4152).
