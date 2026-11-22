<!-- section: Fixed -->
- Python preview no longer reports `exposed` for a changed literal keyword
  argument on a continuation line of a multi-line call (for example
  `stacklevel=3,` in `warnings.warn(`) on the strength of a return-value
  assertion; the line takes the call's shape, as on one line (#16018).
