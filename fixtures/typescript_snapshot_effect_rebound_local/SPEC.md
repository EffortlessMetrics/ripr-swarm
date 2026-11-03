# Fixture: typescript_snapshot_effect_rebound_local

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The test
binds the owner call's result to a local, then rewrites that local through a
destructuring assignment before snapshotting it (#12465):

```ts
let result = enqueue([], 7);
({ result } = { result: ["shipped 7"] });
expect(result).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_rebound_local
```

## Then

The local is read by something other than its declaration and
`expect(result)`, so the allowlist does not read it as the owner result and the
seam reads `weakly_exposed` (RIPR-SPEC-0258 rule 3). Only `expected/check.json`
is retained, for the reason given in
`typescript_snapshot_effect_unrelated_subject`.

## Must Not

- Credit a local whose binding the test rewrites.
