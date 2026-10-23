# Fixture: out_parameter_empty_expected_no_promote

Spec: RIPR-SPEC-0351

Issue: #8999

## Given

The changed method writes through its `&mut String` parameter. Each test
passes a fresh empty local and then asserts it equals a value that may be
empty: `String::from("")`, `String::default()`, `"".to_string()`,
`String::with_capacity(8)`, or an empty local.

## When

```bash
cargo xtask fixtures out_parameter_empty_expected_no_promote
```

## Then

No finding is `exposed`: an expected value that may equal the local's
initial state cannot notice the write, so the out-parameter assertion earns
no oracle credit.

## Must Not

- Credit an out-parameter assertion whose expected value is not clearly
  non-empty.
