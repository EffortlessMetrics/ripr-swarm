# Fixture: typescript_namespace_inactive_pick

Spec: RIPR-SPEC-0243

## Given

The changed predicate includes age 18. A test file picks the skipped Mocha
registration `xit` off a `require('mocha')` namespace and renames it `test`
(`const test = runner.xit`). The extraction guards read the local spelling, so
a renamed skipped test would look active (#10477).

## When

`cargo xtask fixtures typescript_namespace_inactive_pick` analyzes the diff.

## Then

Exactly one finding remains `no_static_path`, with no related tests and
oracle `unknown` / `none`. The honesty corpus pins this independently of the
golden output. Analyzer tests pair every inactive member (`xit`, `xdescribe`)
with its active twin (`it`, `describe`), which stays credited.

## Must Not

- Credit a skipped runner member renamed into an active-looking local.
- Withhold the active twin picked off the same namespace.
- Claim runtime execution or that Mocha skips the test.
