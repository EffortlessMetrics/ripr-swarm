# Fixture: field_match_raw_option_shadow

Spec: RIPR-SPEC-0229

## Given

The owning module declares `const r#None: Option<u128> = Some(1001)`, while
its test module declares the same raw identifier with value `Some(2)`.
Rust treats a raw name as that identifier, so the owner's `None` pattern
and the test's `None` expression resolve to different constants. The changed
first arm selects `Some(1001)`; the passing test takes the later `Some(ms)` arm.

## When

`cargo xtask fixtures field_match_raw_option_shadow`

## Then

Both changed MatchArm subjects stay `weakly_exposed`. Critical raw bindings
must be normalized for shadow detection and never borrow prelude identity.
The independent honesty charter rejects promotion even if expected files
were accidentally re-blessed.

## Must Not

Treat the text `None` as a standard constructor after a raw identifier
replaced its binding, or credit the first arm from a sibling-call assertion.
