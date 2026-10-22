<!-- section: Fixed -->
- TypeScript: a test whose only assertion sits in a `.then` with a rejection
  handler or an async `forEach` callback now reads `static_unknown` with a
  stop reason instead of an actionable `weakly_exposed` gap that asks for an
  assertion the test already has (RIPR-SPEC-0246 rule 4, #9290, #9307).
