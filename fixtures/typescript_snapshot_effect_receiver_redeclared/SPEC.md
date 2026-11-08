# Fixture: typescript_snapshot_effect_receiver_redeclared

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_method_owner_result_control`
(`Outbox.send`). The test constructs an `Outbox`, then an inner block
redeclares the receiver name as a plain object and snapshots that object's
`send` result (#12465):

```ts
const box = new Outbox();
box.send(1);
{
  const box = { send: (id: number) => ["shipped 7"] };
  expect(box.send(7)).toMatchInlineSnapshot(`["shipped 7"]`);
}
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_receiver_redeclared
```

## Then

The receiver name has two declarations, so the allowlist cannot tell which
`box` the assertion reads and the seam reads `weakly_exposed`
(RIPR-SPEC-0258 rule 3). Only `expected/check.json` is retained, for the reason
given in `typescript_snapshot_effect_unrelated_subject`.

## Must Not

- Credit a receiver whose name is declared more than once.
