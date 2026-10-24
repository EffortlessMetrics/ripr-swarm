<!-- section: Changed -->
- Related-test misses: `missing_input` and `missing_exact_assertion` now render
  their specific boundary or value from the test row's own typed support
  record (`RelatedTest.miss_support`, additive and optional in check
  artifacts) and the finding's probe family, never by picking a fact from the
  finding-wide missing-discriminator list. A row without its own support keeps
  the generic wording. Python rows that prove their own boundary omission carry
  the boundary as support; Rust attaches its universal fact to each row it
  assigns the miss; Perl v1 and TypeScript attach none. The `miss` token,
  class, confidence, ranking, actionability, fix-site and finding identity
  are unchanged (#5564).
