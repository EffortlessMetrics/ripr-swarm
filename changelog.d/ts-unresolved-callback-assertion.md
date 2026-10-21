<!-- section: Fixed -->
- TypeScript: a test whose only assertion sits in a `.then` with a rejection
  handler or an async `forEach` callback now reads `static_unknown` instead of
  `weakly_exposed`, so the two false actionable gaps in the TypeScript verdict
  corpus become abstentions (RIPR-SPEC-0246 rule 4). TypeScript false verdicts
  3/180 → 1/180 (#9290).
