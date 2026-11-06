# Fixture: typescript_snapshot_effect_conditional_receiver

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_method_owner_result_control`
(`Outbox.send`). The receiver is chosen by a conditional whose taken arm is an
unrelated object (#12465):

```ts
const fake = { send: (id: number) => ["shipped 7"] };
const box = true ? fake : new Outbox();
expect(box.send(7)).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_conditional_receiver
```

## Then

The receiver initializer is not exactly `new Outbox(..)`, so the receiver is
not read as an instance of the owner's class and the seam reads
`weakly_exposed` (RIPR-SPEC-0258 rule 3). Only `expected/check.json` is
retained, for the reason given in `typescript_snapshot_effect_unrelated_subject`.

## Must Not

- Read a receiver as the owner's class because its initializer mentions `new Outbox(`.
