<!-- section: Fixed -->
- A unique indexed inherent method name no longer grants direct test-call
  attribution without bounded receiver evidence. Reports refuse comment-only,
  expired-binding and assertion-operand item-shadow evidence, dormant nested
  fn, closure and unpolled async bodies, and foreign inherent helper receivers,
  while keeping enum receivers, public-library calls and outer receivers.
  Free-helper entries and hops bind to their own module/function identity
  (#7415, #8004).
- Receiver construction is credited from indexed evidence, not constructor-
  sounding names: `Ty::name()` needs a declaration returning `Self`/`Ty`,
  `Ty::default()` an `impl Default` or `#[derive(Default)]`, `Ty::from(..)` an
  `impl From`, a free `fixture()` a unique declared return type, and a bare unit
  struct a verified declaration. Qualified paths and shadowing locals are
  refused (#7415).
- Accepted cost: verdict-corpus rows `lib-rstest-fixture-injected-receiver`,
  `serde-derive-rename-field-upper` and `serde-derive-rename-variant-lower`
  move from credited/ideal to gap/`false_actionable` on discriminating tests
  (rstest-injected receiver, bare-imported unit variant), a false actionable
  signal accepted in exchange for refusing unproven credit. They are listed
  under `known-false/`; restoring credit is tracked in #8782.
- Source-snapshot facts (parse, lines, declarations, ancestry) and test-module
  shadow answers are reused per run, keeping ambiguous-declaration refusal
  (#7415, #8004).
