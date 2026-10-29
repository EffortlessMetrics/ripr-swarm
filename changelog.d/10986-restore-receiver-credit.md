<!-- section: Fixed -->
- Two of the four Rust verdict-corpus rows #8577 moved to `false_actionable` read
  credited again: an rstest-injected fixture parameter (`fn t(account: Account)`)
  now names its declared owner type, and `Order::default()` is credited by a
  `#[derive(Default)]` declared once in the test's own file even when sibling
  files also declare `Order`. Qualified, generic, shadowed and rebound spellings
  stay refused. `lib-rstest-fixture-injected-receiver` and
  `spec0228-pushed-literal-hand-built-expected` leave `known-false/`. The two
  serde rows remain known-false until #10171's withhold lands (#8782, #10986).
