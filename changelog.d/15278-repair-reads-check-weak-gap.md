<!-- section: Fixed -->
- Bare `ripr repair` no longer says "Nothing to do for this change: the existing
  tests already check it, and `ripr check` reports the same" when `ripr check`
  reads the changed line as weak, unrevealed or unknown. It now names check's
  reading and points to writing or strengthening the test by hand, and it makes
  no claim about check when check could not run (#15278).
