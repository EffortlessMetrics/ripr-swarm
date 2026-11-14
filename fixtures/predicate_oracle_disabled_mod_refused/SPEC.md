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

Exactly one predicate finding reads `reachable_unrevealed`. The module-declaration
refusal names the `#[cfg(any())]` declaration; the strong-looking assertion text
earns no exact-value credit.

## Must Not

- Admit an assertion from a module no build can compile.
- Replace the declaration refusal with weak exact-value evidence.
- Confuse an unevaluable feature atom with the always-false `any()` predicate.
