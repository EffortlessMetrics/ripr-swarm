# Fixture: verification_prelude_glob_plain_import

Spec: RIPR-SPEC-0369

Issue: #14116

## Given

Production `weight` changes from `input * 2` to `input * 3`, and the test pins `weight(4) == 12`. The crate
declares `prusti-contracts` as the plain registry package, locked once at the
audited 0.3.3, so an exact `use prusti_contracts::*;` would be an admitted
verification-prelude glob. But the same file first imports
`shim::prusti_contracts` by name, so in this edition-2015 crate the glob reads
`shim`'s module, whose `vec!` ripr cannot see. The test also calls
`vec!`; the glob cannot rebind `assert_eq!` (rustc E0659, #16060).

## When

`cargo xtask fixtures verification_prelude_glob_plain_import` runs `ripr check`
on the diff.

## Then

The glob stays a foreign glob, so the test's `vec!` call leaves its `assert_eq!` uncredited and
the finding must not read `exposed`.

## Must Not

- Promote the finding to `exposed` because the glob was read as the audited
  `prusti_contracts` prelude.
