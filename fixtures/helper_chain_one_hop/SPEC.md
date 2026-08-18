# Fixture: helper_chain_one_hop

Spec: RIPR-SPEC-0159

## Given

A changed `let` inside a one-hop helper (`is_word_start`) whose only
caller (`classify`) is what the exact-value tests invoke. The tests
never name the helper.

## When

```bash
cargo xtask fixtures helper_chain_one_hop
```

## Then

The helper-owned probe relates both tests through the resolved chain
(`HelperOwnerCall`): reach yes, both exact oracles connected, and the
helper's inputs bound from the call site (`input = " x"` /
`"hello"`), so the boundary machinery evaluates the helper's operands
with the tests' literals.

## Must Not

- Relate tests through the lexical hint alone when the chain stops.
- Claim the caller's predicate or propagation the evidence stages do
  not establish.
