# Fixture: rust_statement_macro_spelling_owner_item

Spec: RIPR-SPEC-0108

## Given

A test file that imports `suit_color`, then at statement position in the test body invokes `emit!`, a macro defined in the library crate and not in the test file, with `fn suit_color(_s: char) -> &'static str { "black" }` in its arguments, and asserts `suit_color('S') == "black"`.

## When

The `'S' | 'C'` arm of the library's `suit_color` changes.

## Then

The changed arm's finding is not `exposed`: the macro's expansion is unread and may put its own `suit_color` in the test body, so the asserted call may not reach the changed owner. Here the expansion does exactly that.

## Must Not

Credit the asserted call to the imported owner because the item is spelled inside a macro's arguments rather than written as a statement item (#8467 finding 3). The honesty corpus independently rejects the false promotion after re-blessing.
