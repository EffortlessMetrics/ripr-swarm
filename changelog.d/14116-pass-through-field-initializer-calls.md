<!-- section: Fixed -->
- A macro call written as a struct field's value inside a `proptest!`
  property (`Check { items: vec![vec![x]] }`, `Check { strategy:
  proptest::prop_oneof![..] }`) no longer makes the property opaque: the
  field's `:` ends the call's path rather than counting as part of it, so
  the call is read as bare or `proptest`-rooted as it is spelled. A call
  under any other path after the colon keeps the workspace-wide refusal
  (RIPR-SPEC-0369, #14116).
