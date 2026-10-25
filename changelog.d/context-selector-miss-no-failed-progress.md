<!-- section: Fixed -->
- `ripr context --at <selector>` with a selector that matches no finding no
  longer prints `ripr progress: failed` before its error; the refusal is
  user-fixable, so only the real error is shown. Genuine failures still print
  `failed` (#9244).
