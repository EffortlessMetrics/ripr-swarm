# Fixture: typescript_snapshot_effect_method_owner_result_control

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A TypeScript class method `Outbox.send` changes the text it pushes onto the
lines it returns (`"sent "` to `"shipped "`), an effect seam. The related test
constructs the class and snapshots the method call's own result (#12465):

```ts
const box = new Outbox();
expect(box.send(7)).toMatchInlineSnapshot(`["shipped 7"]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_method_owner_result_control
```

## Then

The receiver is a local the test constructs from the owner's class and uses
only as `box.<member>(..)`, and the asserted subject is the whole result of the
method call, so the snapshot confirms the effect seam and it reads `exposed`
(RIPR-SPEC-0258 rule 3).

## Must Not

- Refuse a method owner's result because it is called on a receiver.

## Declared classes

```expected-classes
[
  [
    "probe:src_lib.ts:typescript_preview:322bd6d3",
    "exposed"
  ]
]
```
