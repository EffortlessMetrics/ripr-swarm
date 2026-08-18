# Fixture: binding_value_fail_closed

Spec: RIPR-SPEC-0158

## Given

Three changed `let` initializers the evaluator must refuse:
`map_or_else` (unsupported form), a non-identity `map_or` closure, and
a needle that is not an exact input (a third parameter with no observed
literal row).

## When

```bash
cargo xtask fixtures binding_value_fail_closed
```

## Then

No exact boundary observation occurs even though every function has an
exact-input test call: the evaluator refuses each chain by name, the
retargeted predicates keep the honest missing-discriminator weakness
(`observed end values: unknown`), and no `infection yes` boundary fact
appears for these shapes.

## Must Not

- Evaluate `map_or_else`, a non-identity closure, or a dynamic needle.
- Promote an unevaluated chain to an exact value by token resemblance.
