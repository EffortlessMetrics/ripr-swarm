# Fixture: out_parameter_named_empty_no_promote

Spec: RIPR-SPEC-0351

Issue: #10058

## Given

The changed method pushes a nonempty prefix, then calls `push_str` with a
named empty `String`. The test asserts the prefix on the local passed as
`&mut`. Deleting the empty write leaves the asserted output unchanged.

## When

```bash
cargo xtask fixtures out_parameter_named_empty_no_promote
```

## Then

The changed empty write is not `exposed`, and its discriminator is not
`yes`. A bare identifier cannot establish that a text writer adds data.
The honesty corpus independently forbids promotion of this fixture.

## Must Not

- Credit the named empty payload merely because another write produces the
  expected nonempty prefix.
- Infer values of named payloads or expand out-parameter support.
