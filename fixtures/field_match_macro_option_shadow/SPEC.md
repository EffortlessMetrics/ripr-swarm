# Fixture: field_match_macro_option_shadow

Spec: RIPR-SPEC-0229

## Given

An imported statement macro introduces a local constant `None = Some(1001)`.
The test's direct record input therefore selects the `Some` arm, while the
changed `None` arm remains unobserved. Both the original arm and an incorrect
`None => 0` implementation compile and pass the unchanged runtime test.

## When

`cargo xtask fixtures field_match_macro_option_shadow`

## Then

Both changed MatchArm subjects remain `weakly_exposed`. The field selector
must refuse unread statement expansions using existing macro and binding
admission. Ordinary unshadowed standard equality assertions remain supported.

## Must Not

Promote an unselected arm by assuming a prelude constructor remained after an opaque
statement expansion, or treat a rendered golden as independent semantic proof.
