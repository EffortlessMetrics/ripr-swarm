# Fixture: undeclared_target_glob_public_call

Spec: RIPR-SPEC-0197

## Given

The same files as `declared_target_glob_public_call`, except the manifest
declares no target for `xtask/main.rs`. No Cargo target is known to compile
that file, so its placement is unestablished. It holds a glob import. An
integration test calls the changed public owner by its crate-qualified path.

## When

The public analyzer classifies the real wrong-value-to-current-value diff.

## Then

The unplaced file's glob may belong to the test's crate and may bind the
library's name, so the qualified call is not credited and the finding stays
`weakly_exposed` (fail-closed).

## Must Not

- Credit the qualified call while a glob in a file of unestablished placement
  could bind the library's name.
- Infer a declared target from a file's directory name or its `fn main`.
