<!-- section: Added -->
- Verdict corpus: `authored-mined-wire` adds 7 runtime-labeled cases for test
  shapes mined from real crates (bytes, httparse, byteorder) and written as
  our own code: a doc example as the only test and its `ignore`-fenced twin,
  an assert inside a closure in a macro-generated test, a bool-returning
  property checked over a sample loop, a round trip that cannot tell a
  symmetric key apart, and a lookup table with an asserted and an unasserted row.
  Three of the four discriminated cases score `false_actionable` today
  (corpus 2026-10-05.1, 210 cases). Authored false actionable moves from
  56/86 to 59/90 and the overall rate from 0.623 to 0.627; upstream rates are
  unchanged.
