<!-- section: Fixed -->
- `ripr check --format json` TypeScript preview findings now carry a top-level
  `canonical_gap_id` (`gap:typescript:typescript_preview:<fp8>`), the same id
  `ripr agent repair --gap-id` accepts, so agents no longer scrape the human
  output for it (#10709).
