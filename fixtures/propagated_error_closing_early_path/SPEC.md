# Fixture: propagated_error_closing_early_path

Spec: RIPR-SPEC-0197

## Given

A changed closing `)?;` propagates `Failure::Missing` from a fallible
expression. The test asserts the same variant on an earlier return path.

## When

The closing-only diff is analyzed through the current Rust producer.

## Then

The ErrorPath subject is the complete parser-owned `value.ok_or(..)?`
expression at its actual source start. It remains non-exposed because the
assertion does not activate the changed propagation path. The original
removed closing line remains separate base-side evidence.

## Must Not

- Treat shared error-variant tokens as changed-path activation.
- Promote this subject from complete syntax identity alone.
- Hide the propagated-error finding or credit a shadowed assertion.

Contract: [source ripr#1715](https://github.com/EffortlessMetrics/ripr/issues/1715).
