# Fixture: python_factor_reorder_rebound

Spec: RIPR-SPEC-0238

## Given

A parametrized exact-value test rebinds one factor before calling a function
whose return swaps the factors and changes a literal tuple sibling.

## When

The table row supplies 1.5 and 2.0, but the test assigns 9 to the first
factor before the call. Both the old and new results differ from the expected
tuple, so the assertion does not distinguish the edit.

## Then

The finding remains weakly_exposed and names an order-sensitive discriminator.

## Must Not

Credit the unchanged table value after that name has been rebound in the test.
