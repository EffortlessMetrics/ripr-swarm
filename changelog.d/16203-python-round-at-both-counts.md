<!-- section: Fixed -->
- Python preview's call-variant gate reads `round(<parameter>, n)` at both
  counts, by exact decimal half-to-even rounding of a plain decimal literal and
  with the count's sign, negative counts and an edit that adds or removes the
  sign included. An input that both versions round alike now reads
  `weakly_exposed` where counting decimal places called it `exposed`; a count
  past a value's leading digit rounds the value to zero. A float spelling on
  an exact decimal tie is not read, because Python rounds a float by the
  double it names. The gate compares list literals as Python compares them: an
  int spelling beside a float spelling, grouped ints, and strings by their
  spelled bodies. Its first-mention scan abstains where non-ASCII code can
  rename the parameter or the text reader stops short of a statement's end,
  and no longer for non-ASCII text in a string or a comment. The missing check
  it names is a value the two counts round differently, with an example only
  when the same rule finds one, never a wider set such as a number with more
  digits than the smaller count keeps. This follows up #16221 (#16203).
