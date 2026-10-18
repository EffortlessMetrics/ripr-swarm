<!-- section: Changed -->
- `ripr check` classifies the probes of a diff on the rayon pool. The related
  test candidate index shares its lazy memos across workers behind a
  lock, so every worker reads the same warm entries, and the output is
  byte-identical to the serial run (#8756, #8629).
