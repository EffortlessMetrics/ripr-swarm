# Fixture: typescript_snapshot_effect_owner_result_control

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A TypeScript production module changes the text `enqueue` pushes onto the
queue it returns (`"sent "` to `"shipped "`), an effect seam. The related test
snapshots the owner call's own result, which holds the pushed text (#12465):

```ts
expect(enqueue([], 7)).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_owner_result_control
```

## Then

The asserted subject is the whole result of the owner call, and the snapshot
text would change with the edit, so the snapshot confirms the effect seam and
it reads `exposed` (RIPR-SPEC-0258 rule 3). This is the positive control for
the weak fixtures `typescript_snapshot_effect_unrelated_subject`,
`typescript_snapshot_effect_unrelated_committed` and
`typescript_snapshot_effect_rebound_local`.

## Must Not

- Refuse a snapshot whose subject is the owner call result.
