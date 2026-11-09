# Fixture: same_line_call_inputs_nested

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

The inner boundary input is real, but the outer call masks its result. A comment-separated outer call must retain the same refusal. Both old and new nested assertions pass at runtime.

## When

```bash
cargo xtask fixtures same_line_call_inputs_nested
```

## Then

The predicate must remain below `exposed`, retain the real boundary input, and name no missing boundary input. Unsupported observer association remains unconfirmed.

## Must Not

- Borrow an inner or discarded call as the assertion's observed owner result.
- Invent a missing-boundary discriminator when the real input is present.
- Erase an independently observed direct boundary call.
