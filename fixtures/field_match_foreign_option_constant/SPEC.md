# Fixture: field_match_foreign_option_constant

Spec: RIPR-SPEC-0229

## Given

A direct record input supplies the standard `None` to a parameter-field match.
The changed first arm names an out-of-line module constant `foreign::None`,
whose actual value is `Some(1001)`. The existing assertion passes by selecting
the later standard `None` arm and never observes the changed constant arm.

## When

`cargo xtask fixtures field_match_foreign_option_constant`

## Then

The changed MatchArm stays `weakly_exposed`; no selected-field proof may erase
this gap. The field selector retains unresolved pattern authority, so the
legacy qualified-token fallback cannot borrow a same-spelled prelude variant.

## Must Not

Credit the changed arm from the input/expected token `None`, discard a qualified
constant's namespace, or report a clean result because the fixture test passes.
