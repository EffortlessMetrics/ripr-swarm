<!-- section: Fixed -->
- A Python class-attribute message edit now reads `exposed` when the
  assertion that pins the old text spans several lines or is a conjunct of
  `assert a == b and str(exc.value) == "..."`, and an edit to an empty string
  no longer reads as rejecting nothing. An `or` operand, a negation, a call
  argument and an assert message still pin nothing (#15646).
