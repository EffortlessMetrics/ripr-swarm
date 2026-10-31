# Fixture: declared_target_glob_public_call

Spec: RIPR-SPEC-0197

## Given

The package declares a second crate through an explicit `[[bin]]` `path`
(`xtask/main.rs`), a root no autodiscovered layout recognizes, and that file
holds a glob import. An integration test calls the changed public owner by its
crate-qualified path and asserts the exact returned value.

## When

The public analyzer classifies the real wrong-value-to-current-value diff.

## Then

The glob sits in a different crate's module tree, so it cannot bind the
library's name in the test's crate. The exact assertion on the qualified call
is credited and the changed return value is `exposed`.

## Must Not

- Treat a file composed only under another explicitly declared Cargo target as
  a file of the test's crate.
- Weaken the sibling `undeclared_target_glob_public_call` control, where the
  same file is not a declared target and the qualified call stays uncredited.
- Claim a runtime mutation outcome from static evidence.
