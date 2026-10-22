<!-- section: Fixed -->
- Python and TypeScript: a plain test that calls a same-file, non-exported
  assertion helper with literal arguments (`check_pass(50, True)` over
  `assert passes(score) == want`) now reads as the assertions the helper runs,
  so the change is credited instead of reading `no_static_path`. Credit or
  unknown only: cross-file, conditional, loop-nested, non-literal-argument,
  branching or default-parameter helpers, and tests with parameters or local
  bindings, stay unknown. Six Python and six TypeScript `*-grid-*-helper`
  verdict-corpus rows move from abstained to credited, and the per-language
  `missed_credit_ceiling` drops to 25 and 17 (#9636).
