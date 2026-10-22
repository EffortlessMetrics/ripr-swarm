# Fixture: owner_return_pin_pointer_cfg_binding

Spec: RIPR-SPEC-0197, RIPR-SPEC-0108

## Given

A cfg-disabled pointer binding leaves the asserted call bound to a sibling function.

## When

The owner return rewrite is analyzed. The matched integration control compiles,
enumerates and executes one test for both the neutral owner and a wrong return.

## Then

The changed owner's return is not exposed. Both runtime variants pass because
the equality observes the sibling value.

## Must Not

- Borrow the pointer target from another scope, declaration or disabled binding.
- Promote reach or a strong equality without observing the changed owner value.
