<!-- section: Fixed -->
- A test that asserts through a local wrapper no longer reads as "has no
  assertion". An `assert_*` helper call whose arguments contain a macro
  (`common::assert_matches(snapbox::file![..], cmd)`) is now seen as a helper
  call, graded unknown because the macro hides what it compares. A call to a
  same-file `macro_rules!` macro earns an unknown-strength oracle only when
  every arm of its definition asserts one of the arm's own fragments (not a
  tautology, not a self-recursive macro); a macro that only
  logs, a hand-rolled `panic!` check, a call in a closure or branch, a
  duplicate, imported or cfg-gated definition and a macro from another file
  earn nothing (#9122).
