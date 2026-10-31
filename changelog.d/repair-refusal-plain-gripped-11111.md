<!-- section: Fixed -->
- Bare `ripr repair` now says only that a change is already gripped when
  every spot on the changed lines is gripped, instead of also asking for
  evidence or a `tests/` folder for code your change never touched. When
  `ripr check` reads the change as limited or unknown, the refusal no longer
  cites a gripped spot as if it were the verdict on your change: it counts
  those spots as a different check. The not-ready text drops "repair-eligible",
  "(exposed)" and the word "seam" for plain wording (#11111).
