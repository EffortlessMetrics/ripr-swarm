<!-- section: Internal / CI -->
- The Rust verdict corpus gains 45 cases for Rust language shapes and hostile
  source forms, in a new authored crate (`authored-shape-kit`), and a coverage
  gate beyond spec examples. `spec-coverage.toml` now declares `[[axis]]`
  entries and cases carry `axes`; `verdict-corpus validate` and `check` fail
  when an axis holds fewer cases than its floor or a case names an undeclared
  axis. The floors are `test_library_idiom` 59 (the existing `authored-lib-*`
  cases, now tagged), `language_shape` 33 and `hostile_input` 12.
  - Language shapes: operator, `From`, `FromStr`, `Iterator`, `PartialOrd`,
    `Default` and `Drop` impls, `dyn` dispatch, generics and const generics,
    `impl Trait`, associated consts, `let ... else`, labeled `break`, slice
    patterns, casts, saturating arithmetic, closures, async with a hand-written
    executor, threads and `RefCell`.
  - Hostile inputs: assertion text in a comment or a string literal, raw and
    non-ASCII identifiers, an 80-term single-line sum, ten nested parentheses,
    a `macro_rules!`-generated owner, a stacked attribute list on a test,
    a `cfg_attr` test, minified layout, and an owner passed as a function value.

  Every label comes from mutants replayed with `verdict-corpus relabel` (45
  cases, 0 drifted). On this base ripr reads 22 ideally, abstains on 7 and gives
  14 false actionable and 2 false exposed verdicts. Each is recorded in its
  blessed row and filed under #9410 for a fix.
