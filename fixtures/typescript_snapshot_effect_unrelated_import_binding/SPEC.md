# Fixture: typescript_snapshot_effect_unrelated_import_binding

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The test
imports the owner under a rename and imports an unrelated function of the same
name from another module, then snapshots the unrelated function's result
(#12465):

```ts
import { enqueue as real } from "../src/lib";
import { enqueue } from "../src/other";

real([], 7);
expect(enqueue([], 7)).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_unrelated_import_binding
```

## Then

The bare name `enqueue` is bound to another module, so it is not one of the
names the owner is called by in this test and the seam reads `weakly_exposed`
(RIPR-SPEC-0258 rule 3). Only `expected/check.json` is retained, for the reason
given in `typescript_snapshot_effect_unrelated_subject`.

## Must Not

- Credit the owner's original name when an import binds it to another module.
