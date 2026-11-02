# Fixture: typescript_snapshot_effect_unrelated_committed

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The
test runs the owner and compares an unrelated local with a committed Jest
`.snap` entry (`notify 1`) that exists and is the only one (#12465):

```ts
notify(7);
const unrelated = "x";
expect(unrelated).toMatchSnapshot();
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_unrelated_committed
```

## Then

RIPR-SPEC-0258 rule 1 reads the committed entry as `snapshot` / strong, but
the asserted subject is not the owner result, so the effect seam is not
confirmed and the finding reads `weak` (RIPR-SPEC-0258 rule 3).

## Must Not

- Credit a committed entry because it exists.
