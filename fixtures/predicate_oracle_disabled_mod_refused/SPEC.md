# Fixture: predicate_oracle_disabled_mod_refused

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #14537

## Given

`#[cfg(any())]` makes the module declaration provably disabled. The nested test
uses the changed boundary in an ordinary equality, but it can never run.

## When

`cargo xtask fixtures predicate_oracle_disabled_mod_refused` analyzes the diff.

## Then

`src/python.rs` is mounted only by the `#[cfg(any())]` declaration, so rustc
never compiles it and its test is not a related test (#15877). Both findings read
`no_static_path`; the strong-looking assertion text earns no credit.

## Must Not

- Admit an assertion from a module no build can compile.
- Credit the never-compiled test with reach or weak exact-value evidence.
- Confuse an unevaluable feature atom with the always-false `any()` predicate.
