<!-- section: Fixed -->
- Python preview no longer reports `exposed` for an edit that only switches
  `strip`/`lstrip`/`rstrip` on a parameter, or changes the digit count of
  `round(<parameter>, n)` (negative counts included), or flips `reverse=` on
  `sorted(<parameter>)` or `<parameter>.sort()`, when no related test call is
  seen to pass an input the two versions treat differently; it reads
  `weakly_exposed` and names that input (#16203). The gate evaluates a
  `round` literal at both counts instead of counting decimal places, and
  refuses a recursive owner, a module that defines `round` or `sorted`, a
  changed line inside a multi-line string, and literals beyond its rounding
  read (#16221). A body statement holding non-ASCII code withholds the
  prior-use line a sort reads; non-ASCII string or comment text does not.
