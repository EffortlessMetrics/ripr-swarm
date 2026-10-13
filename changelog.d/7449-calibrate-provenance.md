<!-- section: Fixed -->
- `ripr calibrate cargo-mutants` now discloses join provenance on stderr the
  way `ripr outcome` discloses snapshot heads: a headed repo-exposure
  snapshot names the subject the join assumes; a missing or non-full head
  (the usual cargo-mutants case) prints that both artifacts must be from the
  same repository and adjacent commits. `--help` carries the matching
  Limitation. A coincidental file:line from another subject can still join
  (#7449).
