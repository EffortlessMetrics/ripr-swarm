<!-- section: Fixed -->
- `ripr repair` no longer says "existing tests already check it" for a
  predicate whose one-line `if` arms all hold the same statement; the repo seam
  reads propagation as unknown, matching `ripr check` (#12053).
