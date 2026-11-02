# Fixture: typescript_snapshot_effect_owner_result_control

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A TypeScript production module changes the text its `notify` owner logs
(`"sent "` to `"shipped "`), an effect seam. The related test snapshots the
owner call's own result (#12465):

```ts
expect(notify(7)).toMatchInlineSnapshot(`undefined`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_owner_result_control
```

## Then

The asserted subject is the whole result of the owner call, so the snapshot
confirms the effect seam and it reads `exposed` (RIPR-SPEC-0258 rule 3).
This is the positive control for `typescript_snapshot_effect_unrelated_subject`
and `typescript_snapshot_effect_unrelated_committed`.

## Must Not

- Refuse a snapshot whose subject is the owner call result.
