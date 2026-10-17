# Procedural constructor refusal

Spec: RIPR-SPEC-0094

## Given

A derive on test-module `Trigger` emits a local `Bag` factory returning a custom length-comparing field; the owner module declares the standard-vector `Bag`.

## When

The real public check reads the neutral push rewrite (#7966), and the runtime harness compiles the local procedural macro companion before linking each actual test subject against it.

## Then

Both call-deletion and side-effect findings stay `weakly_exposed`. Neutral and wrong-value subjects compile and pass; deletion compiles and fails the named equality. Public and repository evidence retain the actual owner/test target.

## Must Not

- Treat an unrelated standard-vector declaration as the emitted factory's return type.
- Count failed macro setup as a compiling behavioral mutant.
