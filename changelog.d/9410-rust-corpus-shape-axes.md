<!-- section: Internal / CI -->
- The Rust verdict corpus gains 25 cases for Rust language shapes and hostile
  source forms, in a new authored crate (`authored-shape-kit`), and a coverage
  gate beyond spec examples. `spec-coverage.toml` now declares `[[axis]]`
  entries and cases carry `axes`; `verdict-corpus validate` and `check` fail
  when an axis holds fewer cases than its floor or a case names an undeclared
  axis. The floors are `test_library_idiom` 59 (the existing `authored-lib-*`
  cases, now tagged), `language_shape` 13 and `hostile_input` 12.
  - Language shapes: associated consts, casts, const generics, `let ... else`,
    `PartialOrd`, `RefCell`, saturating arithmetic, slice patterns, sort
    direction, thread joins, trait default methods and windowed iteration.
  - Hostile inputs: assertion text in a comment or a string literal, raw and
    non-ASCII identifiers, an 80-term single-line sum, ten nested parentheses,
    a `macro_rules!`-generated owner, a stacked attribute list on a test,
    a `cfg_attr` test, minified layout, and an owner passed as a function value.

  Every label comes from mutants replayed with `verdict-corpus relabel` (25
  cases, 0 drifted). On this base ripr reads 13 ideally, abstains on 2 and gives
  8 false actionable and 2 false exposed verdicts. A second change adds the
  remaining 20 language-shape cases. Each is recorded in its
  blessed row and filed under #9410 for a fix.
