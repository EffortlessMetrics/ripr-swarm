# Fixture: python_assertion_helper_weak_comparison

Spec: RIPR-SPEC-0238

## Given

A counter update observed through a single-comparison helper in the test file.

## When

The update changes from an expanded assignment to `+=`, and `ripr check` reads the helper call.

## Then

The finding is `weakly_exposed`. Deleting the update still satisfies the nonnegative comparison, so the helper stays weak.

## Must Not

Borrow another receiver, field, unresolved helper, or stronger oracle classification.
The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
