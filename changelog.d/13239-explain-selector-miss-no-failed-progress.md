<!-- section: Fixed -->
- `ripr explain <selector>` with a selector that matches no finding (or is
  ambiguous or undecidable) no longer prints `ripr progress: failed` before
  its error; the refusal is user-fixable, so only the real error is shown,
  matching `ripr context`. Genuine failures still print `failed` (#13239).
