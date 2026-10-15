# Macro-generated collection constructor refusal

Spec: RIPR-SPEC-0094

## Given

The owner module declares `Bag(Vec<u32>)`, but a parameterized macro emits the called test-module `Bag` factory returning `Payload(Numbers)`. `Numbers` coerces to `Vec<u32>` and compares only lengths. The macro binds its function name through `$name`, so a lexical `fn Bag` scan cannot establish the actual constructor.

## When

The real public check reads the neutral `v.push(x * 1)` rewrite (#7966).

## Then

Both call-deletion and side-effect findings stay `weakly_exposed`. Neutral and wrong-value subjects compile and pass the named assertion; deletion compiles and fails it. Public and repository evidence retain the actual owner/test target.

## Must Not

- Infer actual field identity from the same-named standard-vector declaration.
- Credit constructor-generating expansion as standard equality evidence.
