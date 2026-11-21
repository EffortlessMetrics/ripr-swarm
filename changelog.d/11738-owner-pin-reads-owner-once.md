<!-- section: Fixed -->
- `ripr check` no longer re-parses a function to find its returned record
  types, or re-reads its closed-form value, for every changed line and repo
  seam that pins its return value. Each analysis worker reads them once per
  batch of changed lines and reuses them for the rest of the batch, so a
  long function with many changed lines pays its length far fewer times
  there. Findings are unchanged (#11738).
