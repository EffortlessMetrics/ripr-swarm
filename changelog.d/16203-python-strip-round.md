<!-- section: Fixed -->
- Python preview no longer reports `exposed` for an edit that only switches
  `strip`/`lstrip`/`rstrip` on a parameter, or changes the digit count of
  `round(<parameter>, n)` (negative counts included), or flips `reverse=` on
  `sorted(<parameter>)` or `<parameter>.sort()`, when no related test call is
  seen to pass an input the two versions treat differently; it reads
  `weakly_exposed` and names that input (#16203). The gate evaluates a
  `round` literal at both counts instead of counting decimal places; a count
  past a value's leading digit rounds the value to zero, and a float spelling
  on an exact decimal tie is not read, because Python rounds a float by the
  double it names. It refuses a recursive owner, an owner whose file rebinds
  `round` or `sorted`, a changed line inside a multi-line string, and literals
  beyond its rounding read; and compares an int spelling with a float spelling
  as Python compares them (#16221). A body statement holding non-ASCII code
  withholds the prior-use line a sort reads; non-ASCII string or comment text
  does not.
