# Fixture: same_line_call_inputs_observed

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

The nested assertion masks the change, but a separate direct boundary assertion actually notices it. Refusing the nested occurrence must retain the independently observed direct call.

## When

```bash
cargo xtask fixtures same_line_call_inputs_observed
```

## Then

The predicate remains `exposed` through the separate direct boundary assertion.

## Must Not

- Borrow an inner or discarded call as the assertion's observed owner result.
- Invent a missing-boundary discriminator when the real input is present.
- Erase an independently observed direct boundary call.
