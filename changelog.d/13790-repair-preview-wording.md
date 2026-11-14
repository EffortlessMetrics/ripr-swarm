<!-- section: Changed -->
- `ripr repair` (and `ripr continue`, which shares the text) no longer says it
  "works from Rust gaps only" for a Python or TypeScript change. In short lines,
  with the root relative to the working directory, it now says the `ripr repair`
  seam flow is Rust only, that preview gaps start with the
  `ripr agent repair --gap-id ...` command `ripr check` prints when one is
  ready, and otherwise how to add the test by hand. When `ripr agent repair
  --phase before` refuses a Python gap as `not_repairable` because the changed
  predicate holds several comparisons, the refusal now names that reason and
  the by-hand test shape, one row per boundary (for example `rate == 0`,
  `rate == 1`). Readiness is unchanged: the shared validator still decides,
  and no start is offered for a gap it refuses (#13790).
