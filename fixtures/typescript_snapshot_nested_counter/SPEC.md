# Fixture: typescript_snapshot_nested_counter

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A named top-level snapshot entry and a second counter assigned inside an executing block.

## When

```bash
cargo xtask fixtures typescript_snapshot_nested_counter
```

## Then

The assertion remains a medium snapshot oracle and the return is `weakly_exposed`.

## Must Not

- Ignore a nested assignment to the same snapshot counter family.
