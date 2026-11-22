<!-- section: Fixed -->
- `ripr pilot`, `ripr repair` and the other whole-repository readings no longer
  say no test reaches a function whose only tests live in another workspace
  package and call it directly (`crates/a/tests/t.rs` calling `b::fee(..)`),
  while `ripr check` names that test. The direct call is credited when the
  function is a free function, the only function of its name in the
  workspace, and the call names it through its own crate (`b::fee(..)`, or
  `fee(..)` under `use b::fee;`). A shared name, or a call spelled through or
  imported from another crate, keeps the package guard (#15278).
