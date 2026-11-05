# Fixture: rust_slice_counter_shadowed_result_constructor

Spec: RIPR-SPEC-0186. Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

The owner counts bytes and apparently returns `Ok(count)` after its loop. A
same-module function named `Ok` instead supplies `Err(TooLong)` at count 17.
The two test assertions use the standard constructors explicitly imported in
their own module and call the owner on separate source lines.

## When

The predicate changes from `count > 17` to `count >= 17`.

## Then

The predicate must remain below `exposed`. Native assertions pass under both
predicates because the shadowed success spelling supplies the firing error.
Removing the shadow preserves the correct predicate's assertions and makes the
wrong predicate fail, so the control distinguishes a masked edit from a real
boundary discriminator.

## Must Not

Grant specialized credit from bare constructor spellings whose retained lexical
bindings are not the standard Result variants. Collapsing both assertions onto
one source line is not equivalent analyzer stimulus: extraction deduplicates
same-name call facts, so this fixture preserves the public regression's layout.
