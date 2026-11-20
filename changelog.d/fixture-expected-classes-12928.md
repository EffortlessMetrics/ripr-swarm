<!-- section: Internal / CI -->
- Machine-check each fixture's stated expected class: a SPEC.md
  `expected-classes` block must agree with `expected/check.json`
  (`check-fixture-contracts`, `goldens check`), `goldens bless` refuses to
  re-pin a contradicting class, and a per-language ratchet policy tracks the
  count (#12928).
