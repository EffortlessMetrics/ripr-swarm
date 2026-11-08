# Fixture: glob_import_pretty_assertions_drop_in

Spec: RIPR-SPEC-0231

Issue: #7641

## Given

Production `weight` changes from `input * 2` to `input * 3`. The only test
imports `helpers::*`, where out-of-line `helpers.rs` exports a
`pretty_assertions` module whose `assert_eq!` macro does not compare. The
crate declares no `pretty_assertions` dependency.

## When

`cargo xtask fixtures glob_import_pretty_assertions_drop_in` runs `ripr check`
on the diff.

## Then

The qualified drop-in assertion earns no strong oracle, so the finding must
not read `exposed`.

## Must Not

- Promote the finding to `exposed` because `pretty_assertions::assert_eq!`
  was read as the registry macro.
