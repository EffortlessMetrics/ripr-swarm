<!-- section: Changed -->
- A TypeScript `missing_input` related-test row now carries the finding's
  changed boundary as its row-owned `miss_support`, so the reason names the
  boundary the test's inputs were not seen reaching (for example
  ``reaching `total == 100` ``) instead of "the changed boundary" (#5495,
  RIPR-SPEC-0224). Class, stages and actionability are unchanged.
