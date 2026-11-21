<!-- section: Fixed -->
- A `proptest!` block (bare through the audited `proptest::prelude::*` glob,
  or as `proptest::proptest!`) or a `serde_json::json!` call that mentions
  `assert_eq`, `vec` or another trusted macro name in its arguments no longer
  refuses assertion credit for every test in the workspace. When the crate is
  the audited registry version and the arguments hold no macro definition,
  module, `extern` or `include`, the possible binding is confined to the
  call's own text. A local `proptest!` (or `prop_assert!` family) definition
  that pastes its arguments keeps the refusal for its crate (RIPR-SPEC-0369,
  #14116).
