# Fixture: rust_statement_macro_spelling_other_item

Spec: RIPR-SPEC-0108

## Given

A test file that imports `suit_color`, then at statement position in the test body invokes `emit!`, a macro defined in the library crate and not in the test file, with `fn other_name(_s: char) -> &'static str { "black" }` in its arguments, and asserts `suit_color('S') == "black"`.

## When

The `'S' | 'C'` arm of the library's `suit_color` changes.

## Then

The changed arm's finding is `exposed`: the macro's arguments spell an item of another name, so the asserted call reaches the imported owner.

## Must Not

Withhold the credit from every test that hands item tokens to a statement macro. This is the control for `rust_statement_macro_spelling_owner_item` (#8467 finding 3).
