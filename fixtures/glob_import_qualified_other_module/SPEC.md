# Fixture: glob_import_qualified_other_module

Spec: RIPR-SPEC-0172

Owner: analysis-fixtures

Issue: #9072

## Given

`pub mod a { pub fn render }` and `pub mod b { pub fn render }` share a
name. The only test imports the crate with `use crate::*;` and calls
`b::render(2)` by its qualified path. The diff rewrites the return of
`a::render` (`x + 1` becomes `1 + x`). `a::render` is never called.

## When

`cargo xtask fixtures glob_import_qualified_other_module` runs `ripr check` on the diff.

## Then

The change reads as `no_static_path`: the glob brings the crate's modules
into scope, so `b::render(..)` names `b::render`, not the changed function,
and `doubles_two` is excluded from the related tests.

## Must Not

- Report `doubles_two` as a test that calls `a::render` because a glob import
  made the qualified call look unresolved.
- Promote the finding to `exposed`.
