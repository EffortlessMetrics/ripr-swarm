# Fixture: out_parameter_unrelated_write_no_promote

Spec: RIPR-SPEC-0351

Issue: #8999

## Given

The changed line writes the other `&mut` parameter and only reads `out`
(`other.push_str(&out.len().to_string())`). The test asserts the local it
passed as `out`, which an earlier, unchanged line writes.

## When

```bash
cargo xtask fixtures out_parameter_unrelated_write_no_promote
```

## Then

The finding is not `exposed` and its discriminator is not `yes`: mentioning
the out-parameter name on the changed line is not a write through it.

## Must Not

- Confirm observation of a change that does not write the asserted parameter.
