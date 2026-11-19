<!-- section: Fixed -->
- `ripr check` reads a function's returned record types and its closed-form
  value once per run instead of re-parsing the whole function for every
  changed line and repo seam that pins its return value. A long function
  with many changed lines no longer pays its length once per line there.
  Findings are unchanged (#11738).
