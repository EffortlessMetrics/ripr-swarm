# Fixture: rust_slice_counter_masked_boundary

Spec: RIPR-SPEC-0186
Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

Adjacent same-element slice lengths 16 and 17 assert different outcomes. A
second branch returns the same error at count 17 independently of the changed
first predicate.

## When

The first predicate changes from `count > 17` to `count >= 17`.

## Then

The predicate remains below `exposed`: both assertions pass under both predicate
versions because the later branch supplies the error when the first does not.

## Must Not

Credit the adjacent lengths as a discriminator merely because their asserted
return spellings differ and the longer one matches the first firing branch.
