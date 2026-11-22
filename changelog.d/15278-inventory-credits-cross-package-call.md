<!-- section: Fixed -->
- `ripr pilot`, `ripr repair` and the other whole-repository readings no longer
  say no test reaches a function whose only tests live in another workspace
  package and call it directly (`crates/a/tests/t.rs` calling `b::fee(..)`),
  while `ripr check` names that test. The direct call is credited when the
  function is a free function and the only function of its name in the
  workspace, the rule `ripr check` already applies. A shared name keeps the
  package guard (#15278).
