<!-- section: Internal / CI -->
- Verdict corpus: `verdict-corpus check` now reports an operator grid (boundary,
  equality, logical, arithmetic, constant, return value and deletion crossed
  with credited, gap and partial runtime truth) and a false-credit trap table,
  gated by a `[grid]` table in a corpus's `spec-coverage.toml` (covered-cell
  floor, trap floor, credited-trap ceiling). The Rust grid gains its missing
  constant cells; the Rust and Perl corpora opt in (RIPR-SPEC-0219, #5503).
