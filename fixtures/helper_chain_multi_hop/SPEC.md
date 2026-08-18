# Fixture: helper_chain_multi_hop

Spec: RIPR-SPEC-0159

## Given

A changed `let` inside a two-hop chain (`classify -> boundary_char ->
first_char`). The test calls only the entry function; the changed
binding lives in the innermost helper and evaluates through the #3295
identity families.

## When

```bash
cargo xtask fixtures helper_chain_multi_hop
```

## Then

The innermost helper-owned probe relates the test through the resolved
two-hop chain with its exact oracle connected, and its operand
evaluates with the entry call's bound input (`input = " x"`), so the
boundary is observed exactly.

## Must Not

- Drop the chain's determinism: the same diff resolves the same hop
  identities and bindings every run.
- Transfer through a hop whose argument is computed rather than a
  literal or parameter.
