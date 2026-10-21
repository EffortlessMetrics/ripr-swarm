<!-- section: Fixed -->
- `ripr agent stub --write` run twice for the same seam now reports
  `Stub already written at <file>` (with the run command; JSON gains
  additive `already_present`, `written` stays `false`) instead of
  suffixing a `_2` duplicate — only a byte-identical recompute counts as
  already written, while a genuinely different test sharing the generated
  name still suffixes (#8891).
- The `strongly_gripped` disclosure on the `ripr agent stub` path names
  the real reason — existing tests already discriminate the seam, so a
  stub would add nothing — instead of reusing the packet/brief "configured
  off" wording; nothing was configured off. A class a user severity
  actually disables keeps the "configured off" wording (#8891).
