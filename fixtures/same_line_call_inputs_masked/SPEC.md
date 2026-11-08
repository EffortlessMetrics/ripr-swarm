# Fixture: same_line_call_inputs_masked

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

A second boundary call appears under multiplication by zero or an ignoring wrapper. The tests execute the input, but their assertions cannot notice its changed return.

## When

```bash
cargo xtask fixtures same_line_call_inputs_masked
```

## Then

The predicate must remain below `exposed`, retain the real boundary input, and name no missing boundary input. Unsupported observer association remains unconfirmed.

## Must Not

- Borrow an inner or discarded call as the assertion's observed owner result.
- Invent a missing-boundary discriminator when the real input is present.
- Erase an independently observed direct boundary call.
