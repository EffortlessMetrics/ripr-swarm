# Fixture: typescript_snapshot_effect_unrelated_subject

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The test
runs the owner but snapshots an unrelated local (#12465):

```ts
enqueue([], 7);
const unrelated = ["shipped 7"];
expect(unrelated).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_unrelated_subject
```

## Then

The snapshot serializes a value the owner did not produce, so it does not
confirm the effect seam. The finding reads `weakly_exposed` instead of
`exposed` (RIPR-SPEC-0258 rule 3). Only `expected/check.json` is retained: the
default human text for a weak effect seam prints a state token that
`check-user-text` counts as a new banned-term line, and that wording belongs to
the existing renderer, not to this change.

## Must Not

- Credit a snapshot because it is spelled as a literal inline snapshot.
- Read an unrelated subject as a side-channel observation of the owner.
