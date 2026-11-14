# Fixture: same_line_call_inputs_negation

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

A custom Not implementation returns a constant. Negation cannot be assumed to preserve the owner result. Old and new runtime variants both observe zero.

## When

```bash
cargo xtask fixtures same_line_call_inputs_negation
```

## Then

The predicate must remain below `exposed`, retain the real boundary input, and name no missing boundary input. Unsupported observer association remains unconfirmed.

## Must Not

- Borrow an inner or discarded call as the assertion's observed owner result.
- Invent a missing-boundary discriminator when the real input is present.
- Erase an independently observed direct boundary call.
