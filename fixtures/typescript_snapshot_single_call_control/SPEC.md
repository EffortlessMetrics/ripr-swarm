# Fixture: typescript_snapshot_single_call_control

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change and committed Jest `.snap` entry (`price 1`) as
`typescript_snapshot_hidden_second_call`, but the test calls
`toMatchSnapshot()` exactly once:

```ts
expect(price(150)).toMatchSnapshot();
```

## When

```bash
cargo xtask fixtures typescript_snapshot_single_call_control
```

## Then

The committed entry is named, so the snapshot oracle reads `snapshot` /
strong (RIPR-SPEC-0258 rule 1). This is the legitimate single-call control
that shows the hidden-second-call fixture is refused because of its second
call, not because the fixture can never promote (#8895).

## Must Not

- Refuse the honest single call.
