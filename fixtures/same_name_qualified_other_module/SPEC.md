# Fixture: same_name_qualified_other_module

Spec: RIPR-SPEC-0172

Owner: analysis-fixtures

Issue: #6292

## Given

`pub mod a { pub fn render }` and `pub mod b { pub fn render }` share a
name. The only test calls `b::render(2)` by its qualified path. The diff
rewrites the return of `a::render` (`x + 1` becomes `1 + x`). `a::render` is
never called, so no mutant of it can fail a test.

## When

`cargo xtask fixtures same_name_qualified_other_module` runs `ripr check` on the diff.

## Then

The change reads as a gap with weak reach: no test is seen calling
`a::render`, and `doubles_two` is listed only because it shares the file.
The qualified call `b::render(..)` names `b::render`, so it is not a direct
call of the changed function.

## Must Not

- Report `doubles_two` as a test that calls `a::render`, or say related tests
  reach `render` (the pre-fix result).
- Promote the finding to `exposed`.
