# Fixture: typescript_snapshot_effect_unrelated_subject

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The
test runs the owner but snapshots an unrelated local (#12465):

```ts
notify(7);
const unrelated = "x";
expect(unrelated).toMatchInlineSnapshot(`"x"`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_unrelated_subject
```

## Then

The snapshot serializes a value the owner did not produce, so it does not
confirm the effect seam. The finding reads `weak` instead of `exposed`
(RIPR-SPEC-0258 rule 3).

## Must Not

- Credit a snapshot because it is spelled as a literal inline snapshot.
- Read an unrelated subject as a side-channel observation of the owner.
