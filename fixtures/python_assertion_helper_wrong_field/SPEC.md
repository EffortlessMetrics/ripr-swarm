# Fixture: python_assertion_helper_wrong_field

Spec: RIPR-SPEC-0238

## Given

A counter update observed through a single-comparison helper in the test file.

## When

The update changes from an expanded assignment to `+=`, and `ripr check` reads the helper call.

## Then

The finding is `weakly_exposed`. The helper reads hits while the changed owner updates writes.

## Must Not

Borrow another receiver, field, unresolved helper, or stronger oracle classification.
The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
