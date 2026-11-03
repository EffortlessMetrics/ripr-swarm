<!-- section: Fixed -->
- TypeScript Vitest `test.for` / `it.for` registrations the extractor cannot
  index are now disclosed through `typescript_test_extraction_partial` instead
  of being silently dropped; empty tables, skipped options and unrelated `.for`
  helpers are unchanged (#12554).
