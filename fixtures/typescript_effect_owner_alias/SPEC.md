# Fixture: typescript_effect_owner_alias

Spec: RIPR-SPEC-0243

## Given

`emitReceipt` changes the audit payload from `amount * 7` to `amount * 9`.
The Node test imports that owner as `discount` and asserts only its unchanged
return value. The independent runtime and public CLI controls for #7514 keep
this return assertion separate from an assertion that captures the audit value.

## When

`cargo xtask fixtures typescript_effect_owner_alias` runs the production CLI.

## Then

Exactly one `side_effect` finding at `src/receipt.ts:2` for `emitReceipt` is
`weakly_exposed`. Reach stays `yes`, with one related test and an
`exact_value` / `strong` oracle. Observation and discrimination stay `weak`:
the assertion does not observe the changed effect. The independent honesty
corpus pins the class, count and oracle without deriving them from this golden.

## Must Not

- Treat a renamed owner return as a separate effect observer.
- Drop the genuine owner relation or downgrade the extracted exact oracle.
- Tighten the conservative substring rule into a whole-identifier or masked
  text rule that grants new effect credit.
- Change value-family credit, relation ranking, the separate changed-token or
  effect-kind rules, TypeScript preview support, or runtime authority.
