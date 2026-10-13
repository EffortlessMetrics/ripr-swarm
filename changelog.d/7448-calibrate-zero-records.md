<!-- section: Fixed -->
- `ripr calibrate cargo-mutants` now fails closed when the importer recognizes
  zero mutant records in valid JSON (`[]`, `{}`, `["hello"]`, or an empty
  `outcomes`/`mutants` list) instead of exiting 0 with "All imported runtime
  mutants matched a static seam." (#7448). The parser's shape heuristics are
  unchanged; the all-matched wording remains for nonempty imports that all
  joined.
