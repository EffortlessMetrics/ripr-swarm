# Fixture: shared_target_assertion_return

Spec: RIPR-SPEC-0169
Spec: RIPR-SPEC-0197

## Given

The library and binary roots declare the same physical module. Library unit
tests and doctests are disabled; the binary defines a no-op `assert_eq!` before
its module declaration. The shared test names the changed owner.

## When

The public analyzer classifies the real wrong-value-to-current-value diff.
Native controls independently execute the enabled binary test with correct
and deliberately wrong values, with standard and no-op macro variants.

## Then

The no-op assertion contributes no credited discriminator for the changed
return value. Agreeing source-role parents remain admitted.

## Must Not

- Infer unique execution context from the representative source-role edge.
- Credit the assertion merely because another target uses the standard macro.
- Change the standard-assertion positive controls or count compile failure as
  a behavioral witness. Runtime outcomes remain separate from static claims.
